# SURVEY SPA — the Concierge overhaul client (#73–#77) — fresh survey 2026-09-25 and on v5 main `2aed9a552`

Read-only survey. v4 read ONLY through `git -C ~/source/quilltap-server show <sha>[:path]`.
Commits, in order: `8bd080267` (#73 phase 1: refusal failover + image trails) → `49059fb14`
(#74 phase 2: refusal ledger + auto-switch) → `4d370a90f` (#75 phase 3: three states) →
`3b463d6b1` (#76 phase 4: the Concierge's own Settings tab) → `ce2f1dabf` (#77 phase 5:
"Try uncensored"). v4 HEAD at survey time is `08c49319d` (two commits past `ce2f1dabf`; not
in scope). Line numbers below are post-commit at the named sha.

Several "client" modules live under v4 `lib/services/**` but are imported by React
components (they are client-safe): `lib/services/dangerous-content/chat-override.ts`,
`concierge-state-presentation.ts`, and (new in #76) `resolver.service.ts`
(`resolveConciergeSettings`, `DEFAULT_CONCIERGE_SETTINGS` — SalonView and chat-settings
`types.ts` import it). They are in scope here; v5's twins are `chat/concierge-state.ts` and
`chat/concierge-state-presentation.ts`.

---

## A. v4 client, per file

### A.1 Salon-side

#### A.1.1 `lib/services/dangerous-content/chat-override.ts` (last touched `4d370a90f`; read at `ce2f1dabf`, 203 lines) — the state derivation (client + server)

Rewritten from four states to three. Exports (verbatim signatures):

- `type ConciergeOverrideValue = 'OFF' | 'UNCENSORED'` (legacy; "No longer written").
- `type ConciergeState = ConciergeMode` = `'moderated' | 'unmoderated' | 'locked'` (from
  `ConciergeModeSchema` in `lib/schemas/chat.types.ts`). "The string values are also the wire
  contract of `conciergeState` on `PUT /api/v1/chats/[id]` and `POST /api/v1/chats`."
- `type ConciergeProvenance = ConciergeModeSetBy | null` (`'operator' | 'concierge' | null`).
- `const CONCIERGE_STATES: readonly ConciergeState[] = ['moderated', 'unmoderated', 'locked']`
  — "Every state, in the order the controls list them."
- `type ChatLike = { conciergeMode?, conciergeModeSetBy?, conciergeModeReason?, conciergeState?,
  conciergeSetBy?, conciergeReason? }` (all `| null`) — reads stored columns FIRST, then the
  server-derived payload keys (so client and server call the same functions).
- `getConciergeState(chat)` (:78-81): `const mode = chat?.conciergeMode ?? chat?.conciergeState;
  return mode === 'unmoderated' || mode === 'locked' ? mode : 'moderated'`. Missing/NULL ⇒
  `'moderated'`. **The legacy pair is ignored entirely** (test "ignores the legacy pair
  entirely").
- `getConciergeProvenance(chat)` (:88-92): `null` when Moderated; else
  `by = conciergeModeSetBy ?? conciergeSetBy`; returns `by` if `'concierge'|'operator'`, else
  **`'operator'`** (unknown/missing provenance on a non-Moderated chat reads as the operator).
- `getConciergeReason(chat)` (:95-98): `null` when Moderated; else
  `conciergeModeReason ?? conciergeReason ?? null`.
- `conciergeStateUsesUncensoredRoute(state)` = `state === 'unmoderated'` (was
  flagged||uncensored).
- `shouldUseUncensoredRoute(chat)` = the above over `getConciergeState`.
- `shouldShowDangerStyling(chat)` = `getConciergeState(chat) === 'unmoderated'` — **"regardless
  of provenance: the provenance goes in the tooltip and helper text, never in colour."**
  (Was `=== 'flagged'`; an operator-set Unmoderated chat IS now painted.)
- `isClassifierOnDuty(chat)` = `=== 'moderated'`.
- `conciergeStateMayFailOver(state)` = `state !== 'locked'`; `mayFailOver(chat)` over it
  (chatless ⇒ Moderated ⇒ true).
- `deriveConciergeModeFromLegacy({conciergeOverride, isDangerousChat})` (:176-190) — table:
  `'UNCENSORED'`→`unmoderated/operator/migration`; `'OFF'`→`locked/operator/migration`;
  NULL+`isDangerousChat===true`→`unmoderated/concierge/classifier`; else
  `moderated/null/null`. `withConciergeModeFromLegacy(chat)` returns unchanged when
  `conciergeMode != null`. (Import/restore only — server lane; not SPA.)

Tests (`__tests__/unit/lib/services/dangerous-content/chat-override.test.ts`): `CONCIERGE_STATES`
"lists the three states in control order"; `getConciergeState` "returns 'moderated' for a
null/undefined chat or a NULL column" / "ignores the legacy pair entirely" / "reads a
server-derived payload (conciergeState) when the column is absent" / "prefers the column over a
derived payload value"; `describe.each(TABLE)('mode=$mode setBy=$setBy')` → derives the state /
derives the provenance / answers shouldUseUncensoredRoute and its state-only twin / answers
shouldShowDangerStyling (provenance never changes the colour) / answers isClassifierOnDuty /
answers mayFailOver and its state-only twin; `getConciergeReason` "is null for Moderated,
whatever is stored" / "returns the stored reason otherwise"; `mayFailOver` "reads a chatless
call as Moderated"; `deriveConciergeModeFromLegacy` it.each; `withConciergeModeFromLegacy` ×2.

#### A.1.2 `lib/services/dangerous-content/concierge-state-presentation.ts` (last touched `3b463d6b1`; 164 lines) — every word/icon/tone

- `type ConciergeTone = 'danger' | 'muted' | 'success'` (`'info'` REMOVED in #76; #75 had kept
  it "for themes that hook it").
- `CHANGE_HINT = "Change it from the Salon sidebar's Chat section."` (unchanged).
- `CONCIERGE_STATE_PRESENTATION` (verbatim):

| state | label | icon | tone | detail |
|---|---|---|---|---|
| `moderated` | `Moderated` | `eye` | `success` | `The Concierge sends everything to the usual providers first, and to the uncensored desk only when one of them refuses. After enough refusals he moves the whole chat himself.` |
| `unmoderated` | `Unmoderated` | `eye-off` | `danger` | `You have opened the uncensored door yourself. Nothing here goes near a moderated provider.` |
| `locked` | `Locked` | `shield` | `muted` | `Only the usual providers, ever. If one refuses, the refusal stands. For the chat that must never reach an uncensored model.` |

  All three `hint: CHANGE_HINT`.
- `NUMBER_WORDS = ['no','one','two','three','four','five','six','seven','eight','nine','ten']`.
- private `conciergeMovedDetail(reason, refusalCount?)`:
  - `reason === 'refusals'`: `n = refusalCount ?? 0`;
    `counted = n > 0 ? \` after ${n < 11 ? NUMBER_WORDS[n] : n} ${n === 1 ? 'refusal' : 'refusals'}\` : ' after the usual providers refused it'`;
    returns `` `The Concierge moved this chat to the uncensored desk${counted}. Set it back to Moderated if you disagree.` ``
  - any other reason (classifier, migration, manual, null): `'The Concierge moved this chat to
    the uncensored desk on reading the conversation. Set it back to Moderated if you disagree.'`
- `conciergeToneSuffix(tone): '' | '-muted'` — `muted`→`'-muted'`, else `''`.
- `conciergeToneTextClass(tone)` — `muted`→`qt-text-muted`, `success`→`qt-text-success`,
  default→`qt-text-danger` (the `info`→`qt-text-info` arm removed).
- `interface ConciergeStateDescription { title; detail; categories: string[] | null; hint }`.
- NEW `interface ConciergeProvenanceNote { setBy?: ConciergeProvenance; reason?:
  ConciergeModeReason | null; refusalCount?: number | null }`.
- **`describeConciergeState(state, provenance: ConciergeProvenanceNote = {}, dangerCategories?)`**
  — SIGNATURE CHANGED: provenance is the NEW 2nd argument, categories moved to 3rd.
  `byConcierge = state === 'unmoderated' && provenance.setBy === 'concierge'`;
  `detail = byConcierge ? conciergeMovedDetail(reason, refusalCount) : presentation.detail`;
  `categories = byConcierge && provenance.reason !== 'refusals' && dangerCategories?.length > 0
  ? dangerCategories : null` (categories surface ONLY for the Concierge's classifier-reason
  move — incl. `migration`/null reason when setBy is concierge; never on refusals, never on
  operator/Locked/Moderated); `title = label`, `hint = presentation.hint`.

Tests (`concierge-state-presentation.test.ts` at `ce2f1dabf`): `CONCIERGE_STATE_PRESENTATION`
it.each / "covers all three states, each with a detail sentence and the same hint" / "keeps the
helper sentences verbatim" / "gives every state a distinct label, icon and tone";
`conciergeToneSuffix` "leaves the danger base rule unsuffixed and names the one modifier" /
"falls through to the base for success (Moderated draws no badge and no mark)" / it.each;
`conciergeToneTextClass` it.each; `describeConciergeState` "reads %s straight off the table with
no provenance" / "uses the operator's sentence for Unmoderated set by the operator" / "names the
refusal count when the Concierge moved the chat after refusals" / "still reads sensibly when the
refusal count is unknown" / "says the classifier's reading when the Concierge moved the chat on
the conversation" / "surfaces categories only for the classifier's own move" / it.each
`['moderated','locked']`.

#### A.1.3 `components/chat/ConciergeMark.tsx` (last `4d370a90f`)

- `ConciergeTooltipBody({title, detail, categories, hint})` — unchanged markup:
  `div.qt-tooltip-body > p.qt-tooltip-title{title} + p{detail} + [div.qt-tooltip-section >
  p.qt-tooltip-section-label "Categories" + p.qt-tooltip-quote{categories.join(', ')}] +
  p.qt-tooltip-hint{hint}`.
- `ConciergeMarkProps { conciergeState: ConciergeState; conciergeSetBy?: ConciergeProvenance;
  conciergeReason?: ConciergeModeReason | null; dangerCategories?: string[]; className?: string }`
  — two NEW props.
- Renders `null` for `'moderated'`. Else `description = describeConciergeState(state, { setBy:
  conciergeSetBy, reason: conciergeReason }, dangerCategories)` — **NO `refusalCount`**: list
  payloads don't carry it, so a Concierge-after-refusals chat's list tooltip reads "…to the
  uncensored desk after the usual providers refused it. Set it back…".
- Classes: `qt-concierge-mark` + (`qt-concierge-mark-muted` for Locked only) + className;
  `<Tooltip placement="top">` over `<span role="img" aria-label={`Concierge: ${label}`}>*</span>`.
  Glyph is always `*`; tone: Unmoderated = base (red), Locked = `-muted` (grey). Unmoderated is
  ONE tone whoever set it.

Tests (`__tests__/unit/components/chat/concierge-mark.test.tsx`): `ConciergeMark` "renders
nothing for Moderated — the default wears no mark" / it.each (state→label/modifier) / "keeps one
tone for Unmoderated whoever set it — provenance is never a colour" / "appends the caller's
classes without losing the tone" / "carries no native title — the drawn tooltip would double up
on it"; `the tooltip`: it.each `['unmoderated','locked']` / "speaks the operator's sentence when
the operator set Unmoderated" / "speaks the Concierge's sentence when the Concierge set
Unmoderated" / "lists the classifier's categories when the classifier moved the chat" / "omits
the categories line when the operator set the state" / "omits the categories line on Locked";
`ConciergeTooltipBody` "renders title, detail and hint, and drops an absent categories line";
`ChatCard — the Concierge mark` "draws no mark for a Moderated chat" / "draws no mark when the
payload carries no state at all" / it.each / "passes provenance through to the mark's tooltip".

#### A.1.4 List payload plumbing: `ChatCard.tsx`, `RecentChatItem.tsx`, `homepage/types.ts`, `lib/chat-utils.ts`, `app/prospero/[id]/types.ts` + `ChatsSection.tsx` (all `4d370a90f`)

- `ChatCardData` gains `conciergeSetBy?: ConciergeProvenance` and `conciergeReason?:
  ConciergeModeReason | null` (comments: "Who put the chat in its state; `null` for Moderated",
  "Why the chat is in its state; `null` for Moderated"); `conciergeState` comment now "The derived
  Concierge state — never a stored column"; `dangerCategories` "shown on the mark's tooltip".
  ChatCard passes both to `<ConciergeMark>` (`className="text-sm flex-shrink-0"`).
- `RecentChat` (homepage) gains the same two keys; RecentChatItem passes them.
- `lib/chat-utils.ts`: `SalonChatShape` + `CharacterChatShape` gain both keys;
  `transformSalonChatToCardData` / `transformCharacterChatToCardData` copy them through.
- `ProjectChat` gains `conciergeSetBy?`, `conciergeReason?` ("Why the chat is in its state;
  `null` for Moderated"); `transformProjectChatToCardData` copies them.
- Homepage tests (`homepage-components.test.tsx`, `RecentChatItem — the Concierge mark`): "draws
  no mark for a Moderated chat" / "draws no mark when the payload carries no state at all" /
  it.each / "gives each state its own tone class" / tooltip: "explains the state after the pointer
  has dwelt on the mark" / "says who opened the door — the Concierge or the operator" / "names the
  classifier's categories when the classifier moved the chat" / "never surfaces a preserved
  category list on an operator state" / "still opens the chat when the mark itself is clicked";
  `RecentChatsSection` "hides the Unmoderated chat and keeps the rest when "Dangerous Chats" is
  on".

#### A.1.5 `components/providers/quick-hide-provider.tsx` (`4d370a90f`)

Comment-only hunk; the rule (`:226-234`) is unchanged code:
`if (shouldHideByIds(chat.characterTags)) return true; if (hideDangerousChats &&
chat.conciergeState && conciergeStateUsesUncensoredRoute(chat.conciergeState)) return true;` —
which now means **Unmoderated only** (either provenance); Moderated and Locked never hidden by the
danger toggle. Tests (`quick-hide-provider.test.tsx`, `QuickHideProvider — shouldHideChat`):
"hides nothing while the toggle is off, whatever the state" / "hides the Unmoderated chat — and
only that — when the toggle is on" / "leaves a chat with no state visible" / "hides by character
tag independently of the danger toggle" / "hides a tagged chat even when its state is one the
danger toggle ignores".

#### A.1.6 `app/salon/[id]/SalonView.tsx` (touched #75, #76, #77; read at `ce2f1dabf`)

- Header pill (`:1190-1222`): `conciergeState = getConciergeState(chat)`; `moderated` ⇒ `null`;
  else `{label, icon, tone} = PRESENTATION[state]`; `description = describeConciergeState(state,
  { setBy: getConciergeProvenance(chat), reason: getConciergeReason(chat), refusalCount:
  chat.conciergeRefusalCount }, chat.dangerCategories ?? undefined)`; `<Tooltip
  content={<ConciergeTooltipBody …/>} placement="bottom"><span className={`qt-danger-badge
  ${suffix ? `qt-danger-badge${suffix}` : ''} flex-shrink-0`} role="img" aria-label={`Concierge:
  ${label}`}><Icon name={icon} className="w-3 h-3" />{label}</span></Tooltip>`. The pill IS the
  one place that passes `refusalCount`. Effect deps switched from
  `isDangerousChat/conciergeOverride` to `conciergeState, conciergeSetBy, conciergeReason,
  conciergeRefusalCount`.
- Sidebar props: `isDangerousChat` prop REMOVED; `conciergeOverride` → `conciergeState=
  {getConciergeState(chat)}` + `conciergeProvenance={{ setBy: getConciergeProvenance(chat),
  reason: getConciergeReason(chat), refusalCount: chat?.conciergeRefusalCount }}`.
- Message list: `isDangerousChat={shouldShowDangerStyling(chat)}` (now Unmoderated) and NEW (#76)
  `conciergeDisplay={resolveConciergeSettings(chatSettings, chat).display}` (see A.1.13).
- #77 "Try uncensored" wiring (`:776-808` area):
  ```ts
  const { retryPicture, retryBackground } = useConciergeRetry(id, fetchChat, startBackgroundPolling)
  const regenerate = regenerationController.regenerate
  const hasChat = !!chat
  const chatIsLocked = getConciergeState(chat) === 'locked'
  const conciergeRetry = useMemo<ConciergeRetryHandlers | undefined>(() => {
    if (!hasChat || chatIsLocked) return undefined
    return {
      onRetryTurn: (messageId) => { void regenerate(messageId, fetchChat, selectSwipeVariant, { url: retryUncensoredTurnUrl(id, messageId) }) },
      onRetryPicture: (toolMessageId) => { void retryPicture(toolMessageId) },
      onRetryBackground: () => { void retryBackground() },
    }
  }, [hasChat, chatIsLocked, id, regenerate, fetchChat, selectSwipeVariant, retryPicture, retryBackground])
  ```
  Passed to `VirtualizedMessageList conciergeRetry={conciergeRetry}`. **Gate = the chat's state
  alone** (not on-duty, not desk configured): absent only on Locked (or no chat). Stable identity
  is a stated requirement ("each transcript row compares these handlers by identity").

#### A.1.7 `app/salon/[id]/types.ts` (#75, #76)

`Chat`: removes `conciergeOverride?: 'OFF'|'UNCENSORED'|null`; adds `conciergeState?:
ConciergeState` ("derived server-side"), `conciergeSetBy?: ConciergeProvenance` ("`null` for
Moderated"), `conciergeReason?: ConciergeModeReason | null` ("(`'manual'`, `'refusals'`,
`'classifier'`, `'migration'`)"), `conciergeRefusalCount?: number` ("Moderation refusals on the
Concierge's ledger since the chat was last Moderated"). Keeps `isDangerousChat?`,
`dangerCategories?`. Re-export `DangerousContentSettings` → `ConciergeSettings` (#76).

#### A.1.8 `components/chat/ChatSidebar.tsx` (#75 + #76; read at `ce2f1dabf`)

- Props: `isDangerousChat?` REMOVED from `ChatSidebarProps`; `conciergeOverride?` →
  `conciergeState?: ConciergeState` ("as the server derived it. Absent reads as Moderated") +
  `conciergeProvenance?: ConciergeProvenanceNote` ("Who set the Concierge state and why — the
  helper text's note"). ParticipantsSection: `isDangerousChat={shouldShowDangerStyling({
  conciergeState: p.conciergeState })}` (participant-card tint now = Unmoderated).
- `ChatSection` concierge block (`:1165-1186`):
  ```tsx
  <label className="qt-label">
    <span className="mb-1 flex items-center gap-1.5">The Concierge <Icon name={presentation.icon} className={`w-3.5 h-3.5 ${conciergeToneTextClass(tone)}`} /></span>
    <select value={conciergeState} onChange=… disabled={conciergeSaving || !conciergeOnDuty} className="qt-select text-sm">
      {CONCIERGE_STATES.map(v => <option key={v} value={v}>{CONCIERGE_STATE_PRESENTATION[v].label}</option>)}
    </select>
    {conciergeOnDuty ? <span className="block mt-1 qt-text-secondary text-xs">{conciergeHelperText}</span>
                     : <ConciergeOffDutyHint className="block mt-1 qt-text-secondary text-xs" />}
  </label>
  ```
  **Flat list, no optgroups**; options text = bare labels `Moderated` / `Unmoderated` / `Locked`
  (no "(default)" in the sidebar). `conciergeHelperText = describeConciergeState(state,
  conciergeProvenance).detail` (provenance-aware incl. refusalCount; no categories used).
- `conciergeOnDuty` (#76): `const { data: conciergeOnDuty = true } = useChatSettingsQuery(
  (settings) => settings.conciergeSettings?.enabled !== false)` — the shared
  `queryKeys.settings.chat` query; defaults to TRUE while loading/absent.
- `handleConciergeStateChange(next)` (`:1097-1123`): `PUT /api/v1/chats/${chatId}` body
  `{ conciergeState: next }`; on `!res.ok` throws `errorData.error || \`HTTP ${status}:
  ${statusText}\``; success toasts **`'The Concierge is on watch'`** (moderated) /
  **`'The uncensored door stands open'`** (unmoderated) / **`'Locked to the usual desks'`**
  (locked); then `onChatUpdated?.()` (parent `fetchChat`); error toast `msg || 'Failed to change
  the Concierge state'`. No query-key invalidation beyond the parent refetch.

#### A.1.9 `components/chat/ConciergeOffDutyHint.tsx` (NEW, `3b463d6b1`, 24 lines)

`export const CONCIERGE_OFF_DUTY_SETTINGS_URL = '/settings?tab=concierge&section=on-duty'`;
renders `<span className={className}>The Concierge is off duty — turn him on in{' '}<Link
href={URL} className="qt-link">Settings → The Concierge</Link>.</span>` — i.e. the text
**"The Concierge is off duty — turn him on in Settings → The Concierge."** Shared by the sidebar
and the New Chat form. "The select is disabled rather than hidden".

#### A.1.10 New Chat: `components/new-chat/NewChatForm.tsx`, `hooks/useNewChat.ts`, `types.ts`, `NewChatModal.tsx`, `app/salon/new/NewChatPageClient.tsx` (#75 + #76)

- `NewChatFormState.conciergeState` default `'moderated'` (`INITIAL_STATE`).
- Form select `#new-chat-concierge` (`:711-728` at `3b463d6b1`): `disabled={creating ||
  !conciergeOnDuty}`, `className="qt-select"`, flat `CONCIERGE_STATES.map(v => <option>{label}{v
  === conciergeDefaultState ? ' (default)' : ''}</option>)` — i.e. "Moderated (default)" normally,
  or "Unmoderated (default)" when `newChatsStartAs === 'unmoderated'` and on duty. Helper:
  on duty ⇒ `<p className="qt-text-xs qt-text-muted mt-1">{conciergePresentation.detail}</p>`
  (plain table detail, no provenance, no hint); off duty ⇒ `<ConciergeOffDutyHint
  className="block qt-text-xs qt-text-muted mt-1" />`.
- New props `NewChatFormProps.conciergeOnDuty?: boolean` (default `true`) and
  `conciergeDefaultState?: ConciergeState` (default `'moderated'`); threaded from `useNewChat`
  by both `NewChatModal` and `NewChatPageClient`.
- `useNewChat` (#76) returns `conciergeOnDuty: boolean`, `conciergeDefaultState:
  ConciergeState`. It reads the SAME chat-settings fetch it already does (for
  `defaultRoleplayTemplateId`):
  `onDuty = settings?.conciergeSettings?.enabled !== false;`
  `conciergeNewChatsStartAs = onDuty && settings?.conciergeSettings?.newChatsStartAs ===
  'unmoderated' ? 'unmoderated' : 'moderated'` ("Off duty the server ignores `newChatsStartAs`
  and every chat is created Moderated"); `setConciergeServerDefault(...)`,
  `setConciergeOnDuty(onDuty)`. Seeding: `conciergeSeededRef` — pre-selects the form ONCE ("later
  reference-data loads (a project change, a cast change) must not undo the user's own choice");
  skipped when `initialConciergeState != null` ("Continuation mode's inherited state outranks the
  global default").
- Create body rule (`:820-829`): `if (state.conciergeState !== 'moderated' ||
  conciergeServerDefault !== 'moderated') requestBody.conciergeState = state.conciergeState` —
  omitted ONLY when the pick is Moderated AND the server default is Moderated; with an
  Unmoderated global default an explicit `'moderated'` is sent.
- Tests: `NewChatForm Concierge picker` (`NewChatForm.test.tsx`): "offers the three states as a
  flat list, no optgroups" / "starts on Moderated and marks it the default" / it.each
  `['moderated','unmoderated','locked']` / "records the chosen state on the form state".
  `useNewChat create request — Concierge state` (`useNewChat.request-body.test.tsx`): "defaults
  to Moderated on the form" / "omits conciergeState entirely when Moderated" / it.each
  `['unmoderated','locked']` (sends it). (No test pins the off-duty/newChatsStartAs arms.)

#### A.1.11 `lib/chat/route-trail-display.ts` (`8bd080267`; unchanged after) + `components/ui/RouteTrailBadge.tsx`

- `RouteTrailRow` gains `profileKind: 'connection' | 'image'` ("Connection profile (the default)
  or image profile") and `label: string` ("What the badge prints beside the provider icon. A
  connection profile is known by its model; an image profile by the name the user gave it").
- `collapseRouteTrail`: new rows set `profileKind: attempt.profileKind ?? 'connection'`,
  `label: attempt.profileKind === 'image' ? attempt.profileName : attempt.modelName`. (Collapsed
  continuation rows keep the first row's profileKind/label.)
- `describeOutcome` refused arm: `evidence === 'inferred' ? ' — inferred' : evidence ===
  'message-pattern' ? ' — by its wording' : ''` → e.g. `refused on content grounds — by its
  wording (content policy)`. `typed-error`, `provider-code`, `finish-reason` add nothing.
- `describeRouteAttempt` unchanged: `` `${profileName} · ${provider}: ${modelName} —
  ${describeVia(via)}; ${describeOutcome(row)}` `` (the hover still prints `modelName`, the
  badge prints `label`).
- `RouteTrailBadge.tsx` one-line: `<ProviderModelBadge modelName={row.label} …>` (was
  `row.modelName`).
- Tests added (`lib/chat/__tests__/route-trail-display.test.ts`, `describe('image trails
  (Concierge overhaul)')`): "labels an image profile by its name and a connection profile by its
  model" (expects `[['image','House Painter'],['image','Kestrel Studio'],['connection',
  'deepseek-chat']]`) / "says when a refusal was read from the wording alone" (expects substring
  `refused on content grounds — by its wording (content policy)`).

#### A.1.12 `components/chat/ToolMessage.tsx` (#73 + #77; read at `ce2f1dabf`)

- Prop `message.routeTrail?: RouteAttempt[] | null` ("The Concierge's call sheet: set on a
  generate_image run whose picture was refused on the way").
- "Tried:" badge (inserted after the header block, before the Tool Request collapsible):
  ```tsx
  {message.routeTrail && message.routeTrail.length > 0 && (
    <div className="mt-1 flex items-center gap-2 qt-text-label-xs" aria-label="Image profiles tried">
      <span>Tried:</span>
      <RouteTrailBadge routeTrail={message.routeTrail} size="xs" />
    </div>
  )}
  ```
  Source = the TOOL row's own `routeTrail` column (written by the image failover chokepoint with
  `profileKind: 'image'` rows). Shown on ANY tool row with a non-empty trail (not gated on
  toolName).
- #77 prop `onTryUncensored?: (toolMessageId: string) => void` ("Absent on a Locked chat (and
  for every other tool)… Offered on refused, failed and delivered pictures alike — a sanitized
  picture is a refusal no detector can see."). Button placed right after "Tried:" and before
  Tool Request:
  ```tsx
  {onTryUncensored && toolData.toolName === 'generate_image' && (
    <div className="mt-2"><button type="button" onClick={() => onTryUncensored(message.id)}
      className="qt-button qt-button-secondary qt-button-sm">Try uncensored</button></div>
  )}
  ```
  Gate: handler present AND `toolName === 'generate_image'` — regardless of success/trail.
- ToolMessage mount sites passing `onTryUncensored={conciergeRetry?.onRetryPicture}`:
  VirtualizedMessageList's standalone TOOL row, and MessageRow's TWO folded-tool sites.

#### A.1.13 `app/salon/[id]/components/MessageRow.tsx` + `VirtualizedMessageList.tsx` (#76 + #77)

- #76: prop `dangerousContentSettings?` → `conciergeDisplay?: ConciergeSettings['display']`
  ("The list passes the plain display (SHOW, no badges) when the Concierge is off duty");
  `dangerDisplayMode` reads `conciergeDisplay.mode`, `showDangerBadges` reads
  `conciergeDisplay?.showWarningBadges !== false`. VirtualizedMessageList takes
  `conciergeDisplay` and forwards it (was `chatSettings?.dangerousContentSettings`). The value is
  `resolveConciergeSettings(chatSettings, chat).display`: exempt chat type or off duty ⇒
  `{ mode: 'SHOW', showWarningBadges: false }`; Locked ⇒ `settings.display`; Unmoderated ⇒
  `{ ...settings.display, showWarningBadges: false }` (**no badges on Unmoderated**); Moderated ⇒
  `settings.display`.
- #77 "Not Dangerous" lifts blur/collapse:
  ```ts
  const hasDangerFlags = !!message.dangerFlags && message.dangerFlags.length > 0
  const hasStandingDangerFlags = !!message.dangerFlags?.some(f => !f.userOverridden)
  const dangerDisplayMode = hasStandingDangerFlags && conciergeDisplay?.mode ? conciergeDisplay.mode : 'SHOW'
  const showDangerBadges = hasDangerFlags && conciergeDisplay?.showWarningBadges !== false
  ```
  Chips still render for overridden flags ("stay (struck through) as the record"). Memo
  comparator adds `prevFlags.some(f => !f.userOverridden) !== nextFlags.some(...)` and
  `prev.conciergeRetry !== next.conciergeRetry`.
- #77 prop `conciergeRetry?: ConciergeRetryHandlers` ("Absent on a Locked chat, which hides every
  such button"). Uses: `onTryUncensored={conciergeRetry?.onRetryPicture}` on both folded
  ToolMessage sites; `onTryUncensored={conciergeRetry?.onRetryTurn}` on MessageActionBar; and the
  Lantern backdrop button after the TerminalEmbed block:
  ```tsx
  {conciergeRetry && isLanternBackgroundRefusal(message) && (
    <div className="mt-2"><button type="button" onClick={() => conciergeRetry.onRetryBackground()}
      className="qt-button qt-button-secondary qt-button-sm">Try uncensored</button></div>
  )}
  ```
- Tests (`__tests__/unit/app/salon/components/MessageRow.concierge.test.tsx`): `"Not Dangerous"
  clears the blur` — "blurs a message with a standing flag" (text `Click to reveal flagged
  content`) / "shows a message whose flags are all overridden, chips still there struck through"
  (chip `NSFW` className contains `line-through`; no `Not Dangerous` button) / "keeps blurring
  while any one flag is still standing" / "lifts the blur when the override lands on a
  re-render"; `"Try uncensored"` — "is offered on a character line and re-rolls it" / "is absent
  on a Locked chat (no handlers)" / "is absent on the operator's own lines" / "is offered on the
  Lantern's refused backdrop and re-queues it" (exactly ONE button on the staff bubble; calls
  onRetryBackground, not onRetryTurn) / "offers the picture retry on a folded generate_image
  block" (TWO buttons: line + picture; tool row has a refused `profileKind:'image'` trail).

#### A.1.14 `app/salon/[id]/components/message-row/MessageActionBar.tsx` (`ce2f1dabf`)

New prop `onTryUncensored?: (messageId: string) => void` ("Offered on character lines only, and
absent on a Locked chat"). Position: immediately AFTER Regenerate, BEFORE Re-attribute (v4 order:
…View source · Edit(user) · Delete · Regenerate · **Try uncensored** · Re-attribute · …):
```tsx
{message.role === 'ASSISTANT' && !message.systemSender && onTryUncensored && (
  <Tooltip content="Try uncensored — regenerate on the Concierge's uncensored desk">
    <button type="button" onClick={() => onTryUncensored(message.id)}
      className="qt-chat-message-action-icon" aria-label="Try uncensored"><Icon name="shield" /></button>
  </Tooltip>
)}
```
Tooltip string verbatim: **`Try uncensored — regenerate on the Concierge's uncensored desk`**;
icon `shield`; aria-label `Try uncensored`.

#### A.1.15 `app/salon/[id]/concierge-retry.ts` (NEW, `ce2f1dabf`, 40 lines — pure)

- `retryUncensoredTurnUrl(chatId, messageId)` =
  `` `/api/v1/chats/${chatId}/messages/${messageId}?action=retry-uncensored&stream=1` ``
- `describeRetryRefusal(error: unknown): string | null`:
  - `'no-understudy'` → **`There is no uncensored desk to send this to — appoint one under
    Settings → The Concierge.`**
  - `'locked'` → **`This conversation is Locked to the usual desks; set it to Moderated should
    you wish the Concierge to take things elsewhere.`**
  - anything else → `null`.
- `isLanternBackgroundRefusal(message: Pick<Message,'systemSender'|'systemKind'>)` =
  `systemSender === 'lantern' && systemKind === 'background-refused'` (systemKind ONLY — no
  content inference, so legacy rows without systemKind get no button).
- `interface ConciergeRetryHandlers { onRetryTurn(messageId); onRetryPicture(toolMessageId);
  onRetryBackground() }`.
- Tests (`__tests__/unit/app/salon/concierge-retry.test.ts`): "builds the narrated retry URL" /
  "words the two refusals and nothing else" / "recognises the Lantern's refused backdrop".

#### A.1.16 `app/salon/[id]/hooks/useConciergeRetry.ts` (NEW, `ce2f1dabf`, 83 lines) + `hooks/index.ts`

`useConciergeRetry(chatId, fetchChat, startBackgroundPolling): ConciergeRetryController
{ retryPicture(toolMessageId): Promise<void>; retryBackground(): Promise<void> }`, memoised.
Both POST `/api/v1/chats/${chatId}?action=retry-image-uncensored` JSON.
- `errorFrom(res, fallback)`: `info = await res.json().catch(() => null)`; `refusal = status ===
  409 ? describeRetryRefusal(info?.error) : null`; returns `refusal || info?.error || fallback`.
- `retryPicture(toolMessageId)`: in-flight Set per tool id ("a second press while it paints is
  lost"); info toast **`The Concierge has taken the commission across the street…`** (U+2026)
  BEFORE the POST; body `{ toolMessageId }`; `!ok` ⇒ throw `errorFrom(res, 'The uncensored desk
  could not produce the picture')`; ok ⇒ `await fetchChat()` then success toast **`The
  uncensored desk has delivered the picture`**; catch ⇒ error toast (message or `'The uncensored
  desk could not produce the picture'`).
- `retryBackground()`: body `{ kind: 'background' }`; `!ok` ⇒ `errorFrom(res, 'Failed to queue
  the backdrop')`; ok ⇒ success toast **`Backdrop commissioned from the uncensored desk`**,
  `notifyQueueChange()`, `startBackgroundPolling()`; catch ⇒ error toast (`'Failed to queue the
  backdrop'`). No in-flight guard.
- Refresh after success: picture ⇒ `fetchChat()` (the new TOOL row arrives by refetch);
  background ⇒ the story-background poll. No TanStack invalidation in either.

#### A.1.17 `app/salon/[id]/hooks/useRegeneration.ts` (`ce2f1dabf`)

`regenerate(messageId, fetchChat, selectSwipeVariant?, options?: RegenerateOptions)`;
`RegenerateOptions { url?: string }` ("Stream from this endpoint instead of the ordinary swipe.
The Concierge's "Try uncensored" uses it: same narration, same swipe, a different desk.").
`fetch(options?.url ?? \`/api/v1/messages/${messageId}?action=swipe&stream=1\`, { method:
'POST' })`. On `!res.ok`: `refusal = res.status === 409 ? describeRetryRefusal(info?.error) :
null; throw new Error(refusal || info?.error || 'Failed to generate alternative response')`.
Everything else (SSE parse, swipe selection, status "Regenerating...") identical to the swipe.

#### A.1.18 `app/salon/[id]/components/system-message-labels.ts` (#73 + #77)

- `KIND_DISPLAY_OVERRIDES` += `refusal: 'provider refusal'` (#73), `'background-refused':
  'backdrop refused'` (#77).
- `inferKindFromContent` (only when `systemKind` is absent): `concierge`: `if
  (c.includes('on grounds of propriety')) return 'refusal'` before `return 'danger'`; `lantern`:
  after `'projected a new backdrop'`→`background`, `if (c.includes('would not take the scene'))
  return 'background-refused'`, before `acting upon the instructions of`.
- `IMPORTANCE_TABLE`: `concierge: { danger: 'high', refusal: 'high', '*': 'high' }`; `lantern:
  { background: 'medium', 'background-refused': 'high', 'character-image': 'medium', image:
  'medium', '*': 'medium' }`.
- Label choice: the persisted `systemKind` wins (writer stamps `'refusal'` on every
  `refusal-rerouted`/`-no-understudy`/`-not-permitted` bubble, `'danger'` on every
  manual/auto transition and classifier verdict, `'background-refused'` on the Lantern refusal);
  so the collapsed bar reads "provider refusal" / "backdrop refused"; manual transitions still
  read the `danger` kind's label.
- Test added (`system-message-labels.test.ts`): "rates the Lantern's refused backdrop high,
  including legacy rows by their wording" (legacy content `The Lantern's usual painter (GOOGLE
  imagen) would not take the scene — called it improper and downed brushes.` ⇒ `high`).

#### A.1.19 The #74 settings number input

Lives ONLY in `components/settings/chat-settings/DangerousContentSettings.tsx` (settings tab),
id `concierge-auto-switch-after-refusals`; superseded in #76 by RefusalsCard (A.2.5). **No
Salon-side placement.** Its #74/#75 strings are dead (file deleted).

#### A.1.20 `app/about/AboutView.tsx` (`4d370a90f`) — About page bullet

Now: `…quick-hide integration, and a three-state per-chat control (Moderated, Unmoderated,
Locked) settable at creation as well as mid-conversation, with the Concierge switching a chat
himself after repeated refusals` (heading "The Concierge – Alternative Content Provision and
Routing" unchanged).

### A.2 Settings-side

#### A.2.1 `app/settings/SettingsView.tsx` (`3b463d6b1`) — the tab list

`SETTINGS_TABS` gains `{ id: 'concierge', label: 'The Concierge', icon: <Icon name="shield" …/> }`
**third**, between `chat` and `appearance`: providers · chat · **concierge** · appearance ·
memory · images · templates · system. `TAB_SUBSYSTEM_MAP.concierge = 'concierge'`;
`case 'concierge': return <ConciergeTabContent />`.

`lib/foundry/subsystem-defaults.ts`: `concierge.description` → **`Who gets asked when the usual
providers refuse, and how flagged content is shown`** (was "Dangerous content detection and
routing settings"), `href: '/settings?tab=concierge'`; `CHILD_SUBSYSTEM_IDS` inserts
`'concierge'` after `'salon'`. `app/foundry/concierge/page.tsx` redirects to
`/settings?tab=concierge` (was `?tab=chat`).

#### A.2.2 `components/settings/tabs/ConciergeTabContent.tsx` (NEW, 123 lines)

- Reads `useSubsystemInfo('concierge')` (→ `info.description`), `useSettingsSection()` (the
  `?section=`), and from `useChatSettingsContext()`: `settings, loading, saving,
  connectionProfiles, imageProfiles, loadingProfiles, handleConciergeUpdate`.
- Loading: `<div className="qt-text-secondary">Loading settings...</div>` (centered `py-8`).
  No settings: `<div className="qt-alert-error">Failed to load the Concierge&apos;s
  settings</div>` → "Failed to load the Concierge's settings".
- Effective object: `{ ...DEFAULT_CONCIERGE_SETTINGS, ...stored, display: {...defaults.display,
  ...stored?.display}, preScreen: {...defaults.preScreen, ...stored?.preScreen} }`.
- Intro: `<p className="qt-text-small qt-text-muted italic mb-6">{info.description}</p>`.
- Off-duty banner when `!enabled`: `div.qt-alert-warning.mb-4 > p.qt-text-small` **`The
  Concierge is off duty. Nothing is rerouted, announced, switched or screened until he is back at
  his post.`**
- Five `CollapsibleCard`s in `space-y-4`, each `forceOpen={activeSection === '<id>'}`:

| # | title | description (verbatim) | sectionId | defaultOpen |
|---|---|---|---|---|
| 1 | `On Duty` | `Whether the Concierge is at his post at all, or has gone off to see a man about a dog.` | `on-duty` | yes |
| 2 | `The Uncensored Desk` | `The less squeamish parties the Concierge sends for when the usual providers clutch their pearls.` | `uncensored-desk` | yes |
| 3 | `When a Provider Refuses` | `How many polite refusals the Concierge endures before he moves a chat along, and how new chats begin.` | `refusals` | yes |
| 4 | `Display` | `Whether flagged content is laid out on the table, draped in gauze, or tucked discreetly behind a curtain.` | `display` | yes |
| 5 | `Pre-Screening (Advanced)` | `An optional doorman who reads every message before it goes in, at the price of a call apiece.` | `pre-screening` | **no** (collapsed) |

- Tests (`__tests__/unit/components/settings/ConciergeTabContent.test.tsx`): "renders the five
  sections under their stable ids" / "keeps pre-screening collapsed by default" (header button
  `aria-expanded="false"`; desk `true`; no scrollIntoView) / "force-opens and scrolls to the desk
  for ?section=uncensored-desk" (scrollIntoView called ONCE on `#uncensored-desk`) / "opens the
  collapsed pre-screen for ?section=pre-screening" / "points at AI Providers and Images when no
  profile is uncensored-compatible" (links `/settings?tab=providers`, `/settings?tab=images`) /
  "lists only uncensored-compatible profiles on the desk, and says nothing about ticking one"
  (Text profile options include `Candid`, exclude `Prim`; crafter first option `Use the cheap
  LLM` and includes `Prim`).

#### A.2.3 `components/settings/concierge-settings/OnDutyCard.tsx` (NEW)

One `SettingsToggleRow` (`checked={settings.enabled}`, `disabled={saving}`, `onChange=v =>
onUpdate({ enabled: v })`), heading **`The Concierge is on duty`**, body **`When a provider
declines a Moderated chat, the Concierge carries the request to the uncensored desk, says so in
the chat, and may move the chat to Unmoderated after repeated refusals. Turn him off and nothing
is rerouted, announced, switched or screened; every chat is answered by its own provider alone,
and the per-chat Concierge select is disabled.`** PUT key: `conciergeSettings.enabled`.

#### A.2.4 `components/settings/concierge-settings/UncensoredDeskCard.tsx` (NEW, 186 lines)

- Props `{ settings, saving, connectionProfiles: ConnectionProfile[], imageProfiles:
  ImageProfile[], loadingProfiles, onUpdate }`. "Always shown, whatever the on-duty switch says,
  so an Unmoderated chat can always name its profile."
- Filters: `compatibleText = connectionProfiles.filter(p => p.isDangerousCompatible)`;
  `compatibleVision = compatibleText.filter(p => p.supportsImageUpload === true)`;
  `compatibleImage = imageProfiles.filter(p => p.isDangerousCompatible)`; `compatibleIds` =
  text ∪ image ids. `disabled = saving || loadingProfiles`.
- `withSelected(list, all, selectedId)`: a stored id no longer on the compatible list is APPENDED
  from `all` (so the select never misreports) — and then gets the suffix
  **` — not marked uncensored-compatible`** (option text suffix when `!compatibleIds.has(id)`).
- `profileLabel(p)` = `` `${p.name} (${p.provider}${p.modelName ? ` • ${p.modelName}` : ''})` ``.
- None-compatible warning (`!loadingProfiles && compatibleText.length === 0 &&
  compatibleImage.length === 0`): `div.qt-alert-warning > p.qt-text-small` **`No profile is
  marked uncensored-compatible yet, so the desk has no one to send for. Tick
  “Uncensored-compatible” on a connection profile in AI Providers or on an image profile in
  Images.`** with `AI Providers` → `/settings?tab=providers` and `Images` →
  `/settings?tab=images` as `qt-link` links (curly quotes are `&ldquo;`/`&rdquo;`).
- `DeskSelect` markup: `label.block.qt-text-label[for=id]{label}` + `select.qt-select#id
  value={value || ''} onChange=v||null` + first `<option value="">{emptyOptionLabel}</option>` +
  profiles + `p.qt-text-small{help}`.

| id | label | help (verbatim) | list | empty option | PUT key |
|---|---|---|---|---|---|
| `concierge-uncensored-text-profile` | `Text profile` | `Answers a chat when its provider refuses, and every turn of an Unmoderated chat.` | `withSelected(compatibleText, connectionProfiles, …)` | `Auto-detect (first uncensored-compatible profile)` | `uncensoredTextProfileId` |
| `concierge-uncensored-image-profile` | `Image profile` | `Paints what the usual image provider refuses to.` | `withSelected(compatibleImage, imageProfiles, …)` | `Auto-detect (first uncensored-compatible profile)` | `uncensoredImageProfileId` |
| `concierge-uncensored-vision-profile` | `Vision profile` | `Describes an attached image when the image-description profile refuses. Must support image attachments.` | `withSelected(compatibleVision, connectionProfiles, …)` | `Auto-detect (first uncensored-compatible vision profile)` | `uncensoredVisionProfileId` |

- Image prompt crafter (NOT a DeskSelect): `label[for=concierge-image-prompt-profile]` **`Image
  prompt crafter`**; `select.qt-select` over **ALL** `connectionProfiles` (no compatibility
  filter); first option **`Use the cheap LLM`**; option text `profileLabel(p)` + (`!p.apiKey ?
  ' ⚠️ No API Key' : ''`); help **`Writes the image prompts for the uncensored desk. Any
  connection profile will do; leave it on the cheap LLM unless that one balks.`** PUT key
  `imagePromptProfileId` (was `cheapLLMSettings.imagePromptProfileId`).
- Closing `div.qt-alert-info > p.qt-text-small` **`Want the warning badges but never an
  uncensored model? Set your chats to Locked, or leave every desk profile on auto-detect and
  untick “Uncensored-compatible” on every profile, so there is no one for the Concierge to send
  for.`**

#### A.2.5 `components/settings/concierge-settings/RefusalsCard.tsx` (NEW)

- Number input `#concierge-auto-switch-after-refusals` (`type=number min=0 max=10 step=1
  className="qt-input w-24" disabled={saving}`), label **`Switch a chat to Unmoderated after this
  many refusals (0 = never)`**, value `settings.autoSwitchAfterRefusals`; onChange `parsed =
  parseInt(v, 10); if NaN return; onUpdate({ autoSwitchAfterRefusals: Math.min(10, Math.max(0,
  parsed)) })` (saves per keystroke, clamped). Help **`Counts only refusals a provider actually
  states on a Moderated chat. When the tally is reached the Concierge moves the whole chat to
  Unmoderated and says so; returning the chat to Moderated clears it. Locked chats are never
  switched.`**
- Select `#concierge-new-chats-start-as`, label **`New chats start as`**, options (`value` /
  `label` / description shown under the select for the selected one):
  - `moderated` / `Moderated` / `New chats go to their own provider first; the Concierge steps in
    only when it refuses.`
  - `unmoderated` / `Unmoderated` / `New chats go straight to the uncensored desk from the first
    message.`
  PUT key `newChatsStartAs`. (Locked is NOT offered — `ConciergeNewChatStateEnum` is
  moderated|unmoderated.)

#### A.2.6 `components/settings/concierge-settings/DisplayCard.tsx` (NEW)

- Select `#concierge-display-mode`, label **`Flagged content`**, options + selected description:
  `SHOW`/`Show`/`Flagged content is shown normally.`; `BLUR`/`Blur`/`Flagged content is blurred
  until you click to reveal it.`; `COLLAPSE`/`Collapse`/`Flagged content is folded away behind a
  placeholder.` → `onUpdate({ display: { mode } })`. (Descriptions CHANGED from the old card's
  "Display flagged content normally with a warning badge" etc.)
- `SettingsToggleRow` heading **`Show warning badges`**, body **`Display category badges on
  flagged messages.`** → `onUpdate({ display: { showWarningBadges } })`.
- Doc comment: "Only consulted while the Concierge is on duty; off duty, everything is shown
  plainly."

#### A.2.7 `components/settings/concierge-settings/PreScreeningCard.tsx` (NEW, 125 lines)

`scansDisabled = saving || !preScreen.enabled`. Controls in order:
1. Toggle heading **`Pre-screen before sending`** → `preScreen.enabled`; body **`Classify
   messages and image prompts before they are sent, and route anything flagged on a Moderated
   chat to the uncensored desk without waiting for a refusal. Costs a classification call per
   item.`** (disabled = saving)
2. `div.qt-text-label` **`What to scan`**, then three toggles (disabled = `scansDisabled`):
   - **`Text chat messages`** / `Classify your messages before they are sent to the LLM.` →
     `preScreen.scanTextChat`
   - **`Image prompts`** / `Classify image generation prompts before expansion.` →
     `preScreen.scanImagePrompts`
   - **`Image generation`** / `Classify the expanded prompt before it is sent to the image
     generator.` → `preScreen.scanImageGeneration`
3. Toggle heading **`Read each chat's summary in the background and switch it when it looks
   dangerous`**, body **`Every ten minutes the Concierge reads the summaries of Moderated chats
   and moves any that read as dangerous to Unmoderated, with an announcement. Locked chats are
   never moved.`** → `preScreen.summaryClassification` (disabled = saving ONLY — independent of
   pre-screen enabled).
4. Range `#concierge-threshold` (`min="0.1" max="1.0" step="0.1" className="qt-range w-full
   max-w-xs"`, disabled = saving), label **`Detection threshold (${threshold.toFixed(1)})`**,
   `onUpdate({ preScreen: { threshold: parseFloat(v) } })`; help **`Lower values flag more
   content; higher values flag only strongly dangerous content.`**
5. Textarea `#concierge-custom-classification-prompt` (`rows=3 className="qt-textarea"`), label
   **`Custom classification prompt (optional)`**, placeholder **`Additional instructions for the
   content classifier...`** (three ASCII dots), `onUpdate({ preScreen: {
   customClassificationPrompt: v || null } })` per keystroke; help **`Appended to the
   classification prompt. Use it to adjust sensitivity for your use case.`**
6. `div.qt-alert-info > ul.qt-text-small.space-y-1.list-disc.list-inside`: **`With an OpenAI
   connection profile, classification uses the free OpenAI moderation endpoint.`** / **`Otherwise
   it falls back to your cheap LLM, at a small cost per item.`** / **`Classification is
   fail-safe: an error never blocks a message.`**

#### A.2.8 `components/settings/chat-settings/types.ts` + `hooks/useChatSettings.ts` (#74, #76)

- `CheapLLMSettings` loses `imagePromptProfileId`; `ChatSettings` loses
  `uncensoredImageDescriptionProfileId` and `dangerousContentSettings`, gains `conciergeSettings?:
  ConciergeSettings`. `DEFAULT_DANGEROUS_CONTENT_SETTINGS` deleted;
  `DEFAULT_CONCIERGE_SETTINGS = SERVER_DEFAULT_CONCIERGE_SETTINGS` (single-sourced from
  `resolver.service.ts`).
- `type ConciergeSettingsUpdate = Partial<Omit<ConciergeSettings,'display'|'preScreen'>> &
  { display?: Partial<ConciergeDisplaySettings>; preScreen?: Partial<ConciergePreScreenSettings> }`.
- `handleConciergeUpdate(updates)`: `current = latest.conciergeSettings ||
  DEFAULT_CONCIERGE_SETTINGS`; `next = { ...DEFAULTS, ...current, ...topLevel, display:
  {...DEFAULTS.display, ...current.display, ...(display ?? {})}, preScreen:
  {...DEFAULTS.preScreen, ...current.preScreen, ...(preScreen ?? {})} }`; `patchChatSettings({
  conciergeSettings: next }, "Failed to update the Concierge's settings", 'Failed to update
  Concierge settings')`. **The PUT always carries the WHOLE `conciergeSettings` object** (server
  `ConciergeSettingsSchema.safeParse` REPLACES the column). Uses `settingsRef` for race safety.
- `patchChatSettings` = `PUT /api/v1/settings/chat`, then `mutateSettings(updatedSettings,
  false)` (seed the cache from the response, no revalidating GET), `showSuccess()`; failure only
  `console.error`.
- Removed handlers: `handleUncensoredImageDescriptionProfileChange`,
  `handleDangerousContentUpdate`.
- Test added (`__tests__/unit/hooks/useChatSettings.test.tsx`): "handleConciergeUpdate
  deep-merges display and preScreen, keeping every other field" — first PUT `{display:{mode:
  'BLUR'}}` ⇒ body `conciergeSettings.display = {mode:'BLUR', showWarningBadges:true}`,
  `preScreen.enabled false`; second `{preScreen:{enabled:true}, autoSwitchAfterRefusals:5}` ⇒
  keeps display, `preScreen` ⊇ `{enabled:true, threshold:0.7, scanTextChat:true}`,
  `autoSwitchAfterRefusals 5`, `enabled true`. The it.each for image-description PUT lost its
  uncensored row.

#### A.2.9 `DangerousContentSettings.tsx` DELETED (385 lines) + `ChatTabContent.tsx` + `ImageDescriptionSettings.tsx`

- ChatTabContent drops the `Dangerous Content` CollapsibleCard (`sectionId="dangerous-content"`)
  and its `imageProfiles` / `handleDangerousContentUpdate` / `handleCheapLLMUpdate` /
  `handleUncensoredImageDescriptionProfileChange` wiring; the tab keeps everything else in order
  (Taboo follows Answer Confirmation directly).
- ImageDescriptionSettings: `onUncensoredProfileChange` prop removed; subtitle now **`When you
  attach an image to a chat with a provider that doesn't support images (like Ollama, OpenRouter,
  etc.), this profile describes it in text.`**; the whole "Uncensored fallback profile" block is
  replaced by `<p className="qt-text-xs">The uncensored fallback, for when this profile refuses to
  describe an image, now keeps company with the Concierge: see its vision profile under <Link
  href="/settings?tab=concierge&section=uncensored-desk" className="qt-link">The Concierge → The
  Uncensored Desk</Link>.</p>`.

#### A.2.10 `lib/help-guide/categories.ts` + `useHelpChatStreaming.ts` (#76)

- `HELP_CATEGORIES` `content-routing` (`label: 'Content Routing (The Concierge)'`): documents
  `['dangerous-content', …]` → **`['the-concierge', 'story-backgrounds',
  'scene-state-tracker']`**.
- `URL_CATEGORY_MAP` inserts `{ pattern: '/settings?tab=concierge', categoryId:
  'content-routing' }` after `templates`, before `images`.
- `labelFromUrl` code UNCHANGED (doc comment example only). Test "labels the Concierge tab":
  `/settings?tab=concierge` → `Settings → Concierge`; `/settings?tab=concierge&section=
  uncensored-desk` → `Settings → Concierge → Uncensored Desk`. categories.test: "should return
  content-routing for /settings?tab=concierge" (also with `&section=uncensored-desk`); "should
  have 11 categories" unchanged.
- Help tree: `help/dangerous-content.md` DELETED, `help/the-concierge.md` ADDED (front matter
  `url: /settings?tab=concierge`), plus edits to ~10 other help pages in #76 and
  `the-concierge.md` again in #77 (server/help-vendor lane; count net 0).

#### A.2.11 CSS `app/styles/qt-components/_chat.css` + `packages/theme-storybook` (#76)

- Deleted rules `.qt-danger-badge-info { --qt-concierge-badge-color: var(--color-info, #2563eb) }`
  and `.qt-concierge-mark-info { --qt-concierge-mark-color: var(--color-info, #2563eb) }` with
  their "Uncensored — the eye you closed yourself." comments; `-muted` comments now "Locked — no
  colour: the chat keeps to the ordinary desks."; header comment rewritten ("the base is the red
  Unmoderated pill; the modifier below recolors it grey for Locked (Moderated renders no badge at
  all)"; mark: "Same two tones… Moderated draws no mark"). `-muted` rules and the base rules
  unchanged. Mirror in `packages/theme-storybook/src/css/qt-components.css`;
  `@quilltap/theme-storybook` 1.0.72 → **1.0.73**.

---

## B. v5 counterparts on main `2aed9a552`

All paths under `apps/web/src/app/` unless noted.

### B.1 Salon-side

| v4 unit | v5 file(s) + lines | what v5 does NOW |
|---|---|---|
| chat-override.ts (A.1.1) | `chat/concierge-state.ts` 1-144 | FOUR-state twin of `60e3c4a0a`: `ConciergeState = 'monitored'\|'flagged'\|'vouched'\|'uncensored'` (:70), `ConciergeChatView {conciergeOverride?, isDangerousChat?}` (:73-76), `getConciergeState` from the legacy pair (:84-88), `conciergeStateUsesUncensoredRoute` = flagged\|\|uncensored (:103-105), `shouldShowDangerStyling` = flagged (:127-131), `isClassifierOnDuty` = monitored\|\|flagged (:141-144). No `CONCIERGE_STATES`, no provenance/reason getters, no `mayFailOver`. **Second `ConciergeState` home:** `core/core-contract.ts:109` (same four literals). |
| presentation (A.1.2) | `chat/concierge-state-presentation.ts` 1-164; oracle `chat/concierge-state-presentation.v4.json` (pin `c43d3b1b4`), recorder `harness/oracle/cases/concierge-presentation.mjs`; spec `chat/concierge-state-presentation.spec.ts` | Four-row table with `info` tone (:39, :62-95), `conciergeToneSuffix` returns `-info` (:103), `conciergeToneTextClass` `qt-text-info` (:116-121), `describeConciergeState(state, dangerCategories)` TWO-arg, categories only on `flagged` (:148-163). No `ConciergeProvenanceNote`, no `conciergeMovedDetail`. |
| ConciergeMark (A.1.3) | `chat/concierge-mark.ts` 1-130 (`ConciergeTooltipBody` :23-43, `conciergeMarkClasses` :55-62, `ConciergeMark` :88-130); spec `chat/concierge-mark.spec.ts` | Inputs `conciergeState`, `dangerCategories`, `className` only; hides on `'monitored'` (:94); `describeConciergeState(state, cats)` (:119-121). Needs `conciergeSetBy`/`conciergeReason` inputs + 3-arg call. |
| ChatCard / list data (A.1.4) | `screens/salon/chat-card.ts` (mark :102-106, input `EnrichedChatSummary` :179); `screens/salon/salon-list.ts` (quick-hide :170-185); `screens/prospero/cards/project-chats-section.ts` (reuses `qt-chat-card` over `EnrichedChatSummary` :77/:102, quick-hide :115-128); `screens/home/recent-chat-item.ts` (mark :48-52, input `RecentChat` :60), `screens/home/home.api.ts` :28-31 (`RecentChat.conciergeState?`, `dangerCategories?`), `screens/home/recent-chats-section.ts` :68; `screens/characters/view/tabs/character-conversation-card.ts` (mark :98-102, input `CharacterChatSummary` :155), `conversations-tab.ts` :177 | v5 has NO `ChatCardData`/`chat-utils` transform layer — cards bind the DTOs directly. Need `conciergeSetBy`/`conciergeReason` on `EnrichedChatSummary` (core-contract :2936-2944), `CharacterChatSummary` (:3661-3664), `RecentChat` (home.api.ts :28-31), then pass to `<qt-concierge-mark>` at the three mark sites. |
| quick-hide (A.1.5) | `quick-hide/should-hide.ts` :56-64 (rule), `quick-hide/quick-hide.service.ts` :119 | Same rule shape, delegating to `conciergeStateUsesUncensoredRoute` — becomes correct automatically once that predicate is three-state (Unmoderated only). Specs `quick-hide/quick-hide-consumers.spec.ts` pin four-state. |
| SalonView pill (A.1.6) | `chat/conversation-header.ts` (pill template :60-86, `conciergeState` :159, presentation :162-164, `conciergeDescription` :172-174, `badgeSuffixClass` :185-188); spec `chat/conversation-header.spec.ts` (:274, :302 pin `qt-danger-badge-info`) | Pill hidden on `'monitored'`; description has no provenance/refusalCount; reads `getConciergeState(this.chat())` over the legacy pair. |
| SalonView sidebar/list wiring (A.1.6) | `screens/salon/salon-conversation.ts` :353-354 (`[isDangerousChat]="c.isDangerousChat === true"`, `[conciergeOverride]="c.conciergeOverride ?? null"` on `<qt-chat-sidebar>`), :425 + :2065-2067 (`isDangerousChat = computed(() => shouldShowDangerStyling(this.chat()))` → message list) | Wires the legacy pair. No `conciergeDisplay`, no `conciergeRetry`. `chatQuery` :1146; `chatSettingsKeys.all` query already present at :1162. |
| Salon `Chat` type (A.1.7) | `core/core-contract.ts` `ChatDetail` :3323-3326 (`isDangerousChat: boolean \| null`, `dangerCategories: string[]`, `conciergeOverride: 'OFF'\|'UNCENSORED'\|null`) | No `conciergeState/SetBy/Reason/RefusalCount`. |
| ChatSidebar (A.1.8) | `chat/sidebar/chat-sidebar.ts` (inputs `isDangerousChat` :378, `conciergeOverride` :390, `participantDangerStyling` :397-403, bindings :255/:293-294); `chat/sidebar/participants-section.ts` :87/:138; `chat/sidebar/participant-card.ts` :427-428/:500; `chat/sidebar/chat-section.ts` (toasts `CONCIERGE_TOASTS` :41-46, template :139-171 with the two optgroups :161-168, inputs :380-381, `afterRenderEffect` select re-write :478-483, `conciergeState` computed :636-641, helper :654-665, `onConciergeStateChange` :678-700 dispatching `chatUpdate { chat: {}, conciergeState }`); spec `chat/sidebar/chat-section.spec.ts` (:556 pins `qt-text-info`) | Four options in two optgroups; helper = plain table detail; no on-duty gate; four toasts (`Marked as flagged`, `You have vouched for this chat`). The P4.D141 select re-write idiom (afterRenderEffect) is reusable. |
| ConciergeOffDutyHint (A.1.9) | NONE | — |
| New Chat (A.1.10) | `screens/new-chat/new-chat.types.ts` :106-112 (field doc) / :142 (`conciergeState: 'monitored'` default); `screens/new-chat/new-chat-form.ts` template :203-244 (optgroups :234-241, "Monitored (default)"), computeds :466-484, `chatSettings` query `['chatSettings']` :497-500, `onConciergeState` :822-830; `screens/new-chat/new-chat.logic.ts` :214-220 (omit when `'monitored'`); `screens/new-chat/new-chat.state.ts` :267-340 (already fetches `chatSettings` for `defaultRoleplayTemplateId` — the natural seed point for `newChatsStartAs` / on-duty); `core/core-contract.ts` `ChatCreateRequest.conciergeState` doc :896-908 | Four-state form; no on-duty, no `(default)` follow, no one-shot seed, no "send explicit moderated when default is unmoderated" rule. v5 has NO continuation-concierge source (new-chat.state.ts :671 comment) — v4's `initialConciergeState` precedence arm has nothing to bind to. |
| route-trail-display (A.1.11) | `chat/route-trail-display.ts` (evidence :39-40, collapse :60-85, refused arm :118-119); `chat/route-trail-badge.ts` (passes `row.modelName` :45, :53); oracle `chat/route-trail-display.oracle.spec.ts` over `testing/fixtures/route-trail-display.oracle.ndjson`, recorder `apps/web/oracle/route-trail-display.ts` (pin `78b381a96`); `core/core-contract.ts` `RouteAttempt` :3146-3160 (`evidence?: 'finish-reason' \| 'inferred'`, no `profileKind`) | Pre-#73: no `profileKind`/`label`, no `— by its wording`, evidence union two-wide. |
| ToolMessage "Tried:" + Try uncensored (A.1.12) | `chat/tool-message.ts` (inputs `message: MessageDto`, `chat: ChatDetail`, `embedded` :378-380; no routeTrail rendering, no retry output); mount sites `chat/message-list.ts` :117, :157; `chat/message-row.ts` :325 (folded, ONE site in v5 vs TWO in v4) | `MessageDto.routeTrail` (core-contract :3189) is already present on every row incl. TOOL, so "Tried:" is purely SPA work once the server writes image trails. |
| MessageRow danger display + Not Dangerous (A.1.13) | NONE | **Confirmed: v5 has no danger-flag UI at all** — no `dangerFlags` on `MessageDto`, no `DangerFlagBadge`, no blur/collapse, no "Not Dangerous", no `override-danger-flag` verb (grep of `apps/web/src` and `crates/*/src/api` empty; `dangerFlags` exists only in core DB/orchestrator code). `conciergeDisplay` would have no consumer. |
| MessageRow "Try uncensored" (A.1.13) | `chat/message-row.ts` (staff-row header :132-170, images :284, folded tools :318-330, action bar :330-470: Regenerate :383-393, Re-attribute :395-407, LLM logs :409-419, Delete :426-435) | No `conciergeRetry` input, no Lantern backdrop button, no action-bar entry. NB v5's action-bar order differs from v4 (Delete placed after LLM logs); v4 puts Try uncensored right after Regenerate. |
| concierge-retry.ts / useConciergeRetry (A.1.15-16) | NONE | Toast service `ui/toast.service.ts`; `notifyQueueChange` from `layout/queue-status.logic`; background poll = `screens/salon/salon-conversation.ts` `StoryBackgroundPoller` :1281 + `onRegenerateBackground` :1319-1335 (dispatch `chatRegenerateBackground`, core-contract :6493). |
| useRegeneration `url` option (A.1.17) | `chat/regeneration.state.ts` 1-211 (`regenerate(messageId, refetch, selectSwipeVariant?)` :128-209; dispatches `{ type: 'messageSwipe', messageId, stream: true }` :179; frames via `isSwipeProgressEvent(frame, messageId)` :145; error `FAILED = 'Failed to generate alternative response'` :25); Salon caller `screens/salon/salon-conversation.ts` `onRegenerate` :4251-4272 | No options arg; v5 streams over the Event channel (`swipeProgress` frames), not SSE — the retry needs a NEW dispatch verb (or a `messageSwipe` flag) whose frames the same subscription can read. Errors arrive as `CoreDispatchError {kind, message, code?}` (core-contract :4246-4266, :4471). |
| system-message-labels (A.1.18) | `chat/system-message-labels.ts` (`KIND_DISPLAY_OVERRIDES` :14-68, ends `timestamp: 'time'` :67; lantern inference :111-114; concierge inference :119-120; importance `concierge` :293, `lantern` :294); spec `chat/system-message-labels.spec.ts` | Neither `refusal` nor `background-refused` present. |
| About bullet (A.1.20) | `screens/about/about-page.ts` :346 (four-state sentence); spec `screens/about/about.spec.ts` | Old sentence. |

### B.2 Settings-side

| v4 unit | v5 file(s) + lines | what v5 does NOW |
|---|---|---|
| Tab list (A.2.1) | `screens/settings/settings.ts` (template `@switch` :58-84, `tabs` :105-113, `subsystemMap` :116-124, `section` input accepted-but-unused :98-103); `?tab=` routing via `qt-entity-tabs [defaultTab]` + `queryParamMap`; each tab reads its own `?section=` (`chat/chat-tab.ts` :306 etc.) | Seven tabs; no `concierge`. No subsystem-defaults/Foundry module in v5 (no `/foundry/concierge` route, no `useSubsystemInfo` description source — tabs hard-code their intro, e.g. chat-tab :90-94). |
| ConciergeTabContent + 5 cards (A.2.2-7) | NONE (nearest analogue: `screens/settings/chat/dangerous-content-settings.ts` 1-444) | v5's old card: mode select (OFF/DETECT_ONLY/AUTO_ROUTE) gating everything, two uncensored pickers gated on AUTO_ROUTE, display mode + badges, custom prompt, image-prompt picker writing `cheapLLMSettings.imagePromptProfileId` (:309-313, :400-401, :437-441). No `autoSwitchAfterRefusals` (#74 never ported). Base class `ChatSettingsCard` (`chat/chat-settings.api.ts` :48-94): per-card `saving`, `save()` → dispatch `chatSettingsUpdate` → `setQueryData(['chatSettings'])`, visible `qt-error-alert` on failure (a stated v5 departure). Toggle row idiom: `<label class="qt-settings-toggle-row">` (e.g. `composer-emoji-settings.ts` :31). `ui/collapsible-card.ts` has `forceOpen`. |
| Chat tab's Dangerous Content card (A.2.9) | `screens/settings/chat/chat-tab.ts` import :27, card :241-248 (`sectionId="dangerous-content"`) | Still mounted. |
| types (A.2.8) | `screens/settings/chat/chat-settings.types.ts` :200-265 (`DangerousContentSettings`, `DANGEROUS_MODE_OPTIONS`, `DANGEROUS_DISPLAY_MODE_OPTIONS`, `DEFAULT_DANGEROUS_CONTENT_SETTINGS`), `CheapLLMSettings.imagePromptProfileId` :282; `core/core-contract.ts` `ChatSettingsDto.dangerousContentSettings` :3731-3735 | Old bag. |
| ImageDescriptionSettings (A.2.9) | `screens/settings/chat/image-description-settings.ts` (subtitle :38, fallback picker :78-100, `fallbackId` :130-132, save `{ uncensoredImageDescriptionProfileId }` :151) | Old two-picker card. |
| cheap-LLM bag writer | `screens/settings/providers/cheap-llm-card.ts` :244-266 (`merged = { ...this.cheap(), ...patch }` → `chatSettingsUpdate { cheapLLMSettings: merged }`) | Spreads WHATEVER the GET returned into the PUT — see Trap D.5. |
| help categories (A.2.10) | `help/help-categories.ts` :146-148 (`documents: ['dangerous-content', …]`), `URL_CATEGORY_MAP` :152-163 (no concierge row; images at :155); fixtures `help/__fixtures__/help-guide-tables.json` (:139-159), `help/__fixtures__/label-from-url-vectors.json` (:60 `/settings?tab=chat&section=dangerous-content`); recorder `apps/web/oracle/help-guide-capture.test.tsx` (35 LABEL_URLS, section C); `help/help-stream.ts` `labelFromUrl` :91 | Old slug; no concierge URL row; no concierge label vectors. `labelFromUrl` itself needs no change. |
| CSS (A.2.11) | `apps/web/src/styles/qt-components/_chat.css` :3044-3087 (`.qt-danger-badge-info` :3062-3065, `.qt-concierge-mark-info` :3084-3087, four-state comments :3045-3047/:3057/:3079) | `-info` rules still present. No theme-storybook mirror in v5. |

### B.3 Specs that pin the CURRENT four-state behaviour (will move)

Unit (`*.spec.ts`): `chat/concierge-state.spec.ts`, `chat/concierge-state-presentation.spec.ts`
(+ `.v4.json`), `chat/concierge-mark.spec.ts` (`-info` rows :93, :110, :185, :190),
`chat/conversation-header.spec.ts` (:274, :302), `chat/sidebar/chat-section.spec.ts` (:556
`qt-text-info`), `chat/message-list.spec.ts`, `chat/message-row.spec.ts`,
`chat/chat-view-model.spec.ts` (:254 `conciergeOverride: null` fixture),
`chat/merge-conversation-modal.spec.ts` (:52 fixture), `quick-hide/quick-hide-consumers.spec.ts`,
`screens/home/home.spec.ts`, `screens/salon/chat-card.spec.ts`, `screens/salon/salon-list.spec.ts`,
`screens/salon/salon-conversation.spec.ts` (pins the `[conciergeOverride]` wire),
`screens/salon/salon-turn-controls.spec.ts` (:155 fixture), `screens/salon/salon-settings-live.spec.ts`
(:117 fixture), `screens/salon/salon-impersonation-voice.spec.ts`,
`screens/prospero/cards/project-chats-section.spec.ts`,
`screens/characters/view/tabs/conversations-tab.spec.ts`, `screens/new-chat/new-chat-form.spec.ts`,
`screens/new-chat/new-chat.logic.spec.ts`, `screens/about/about.spec.ts`,
`screens/settings/chat/async-select-cards.spec.ts`,
`screens/settings/chat/connection-profiles-shared-entry.spec.ts` (dangerous-content card),
`workspace/chrome/link-interceptor.spec.ts` (:217 `conciergeState: 'monitored'` fixture),
`help/help-categories.spec.ts`, `help/help-stream.spec.ts` (fixture-driven),
`chat/route-trail-display.spec.ts` + `.oracle.spec.ts`, `chat/system-message-labels.spec.ts`.
(`chat/confirmation-badge.ts` "Vouched" is the ANSWER-confirmation badge — unrelated; do not
touch.)

e2e (`apps/web/e2e/`): `salon-concierge-four-state-flow.spec.ts` (ten transitions over the
stored `(conciergeOverride, isDangerousChat)` pair via CLI SQL, five manual phrases — rewrite
wholesale, model on v4's renamed `scripts/concierge-three-state-test.sh`),
`concierge-marks-flow.spec.ts` (four states, `-info` modifier :65, the four detail sentences
:70-77), `salon-danger-avatar-flow.spec.ts` (asserts an operator-Uncensored chat is NOT ringed
and reads the stored pair — **both inverted/obsolete**: operator Unmoderated IS ringed now, and
`conciergeOverride` is dropped), `salon-streaming-avatar-flow.spec.ts` (dispatches
`conciergeState: 'monitored'`/`'flagged'` :148, :191, :258 — will 400),
`new-chat-flow.spec.ts` (:200-270 "Monitored (default)", picks `flagged`),
`settings-chat-cards-flow.spec.ts` (:108 card order incl. `Dangerous Content`, :144-183 the
dangerousContentSettings bag round-trip — must move to the Concierge tab).

### B.4 Recorded SPA oracle fixtures to re-record

- `chat/concierge-state-presentation.v4.json` ← `harness/oracle/cases/concierge-presentation.mjs`
  (runs v4's real module via Node 24 type-stripping; still viable — the new imports are
  `import type` only). Must grow provenance shapes (operator / concierge×{refusals n=0,1,2,11,
  null; classifier; migration}; categories on/off) and drop `info`.
- `testing/fixtures/route-trail-display.oracle.ndjson` ← `apps/web/oracle/route-trail-display.ts`
  (add `profileKind: 'image'` rows, the five evidence values incl. `message-pattern`, the `label`
  field in collapse output, and the widened enum rows it already carries for via/outcome/trigger).
- `help/__fixtures__/help-guide-tables.json` + `label-from-url-vectors.json` ←
  `apps/web/oracle/help-guide-capture.test.tsx` (add the two concierge URLs to LABEL_URLS; the
  tables pick up the slug swap and the new URL row).
- No recorder exists for `chat-override` (v5's `concierge-state.spec.ts` is a hand
  transcription) nor for `system-message-labels` or `chat-utils`.

---

## C. The wire contract the client consumes

| verb (v4 REST) | v5 dispatch today | request | response keys the client reads | change |
|---|---|---|---|---|
| `GET /api/v1/chats/[id]` | `chatGet` → `ChatDetail` | — | NEW `conciergeState: 'moderated'\|'unmoderated'\|'locked'` (=`getConciergeState(row)`), `conciergeSetBy: 'operator'\|'concierge'\|null` (null when Moderated), `conciergeReason: 'manual'\|'refusals'\|'classifier'\|'migration'\|null` (null when Moderated), `conciergeRefusalCount: number` (`repos.chats.getModerationRefusalLedger(chatId).count`); KEPT `isDangerousChat` (telemetry only), `dangerCategories`; REMOVED `conciergeOverride` | #75 (`handlers/get.ts`). ⚠ Keys are `conciergeSetBy`/`conciergeReason`/`conciergeRefusalCount` — NOT `conciergeStateSetBy`/`conciergeStateReason`/`moderationRefusalCount` (`moderationRefusalCount` is the DB column name, never on the wire). |
| `PUT /api/v1/chats/[id]` | `chatUpdate { chatId, chat: {}, conciergeState }` | top-level `conciergeState: ConciergeModeSchema.optional()` = `'moderated'\|'unmoderated'\|'locked'`; the retired four values → **400** | `error` on failure; client refetches the chat | #75 (`[id]/schemas.ts`). Applied through `applyConciergeFlip`, which posts the transition bubble (see kinds below). |
| `POST /api/v1/chats` (create) | `chatCreate` | `conciergeState?: 'moderated'\|'unmoderated'\|'locked'`. Absent ⇒ `newChatsStartAs` when on duty (else none). Present non-`moderated` while OFF DUTY ⇒ **ignored** (WARN, not 400) | `{ chat: { ...row, conciergeMode, conciergeModeSetBy, conciergeModeReason, participants } }` — the RAW column names, post-flip | #75/#76 (`app/api/v1/chats/route.ts` :138-149, :408-445, :1453-1520). |
| List payloads: `GET /api/v1/chats` (enrichChatForList: Salon list, home recents), `GET /api/v1/projects/[id]?action=chats`, `GET /api/v1/characters/[id]` chats | `chatsList` / `projectChats` / character chats → `EnrichedChatSummary`, `CharacterChatSummary`, `RecentChat` | — | `conciergeState` (derived), NEW `conciergeSetBy`, NEW `conciergeReason`, `dangerCategories` (`[]` when none). NO refusal count. | #75 (`chat-enrichment.service.ts`, `projects/[id]/actions/chats.ts`, `characters/[id]/handlers/get.ts`). |
| `GET /api/v1/settings/chat` | `chatSettings` → `ChatSettingsDto` | — | NEW `conciergeSettings` (below); REMOVED `dangerousContentSettings`, `uncensoredImageDescriptionProfileId`, `cheapLLMSettings.imagePromptProfileId` | #76. |
| `PUT /api/v1/settings/chat` | `chatSettingsUpdate { settings }` | `conciergeSettings` = the WHOLE object (safeParse fills defaults; invalid ⇒ throws `Invalid conciergeSettings: <path>: <msg>; …` → 400 per the test "rejects a malformed conciergeSettings with 400"). **Any of `dangerousContentSettings`, `uncensoredImageDescriptionProfileId`, `cheapLLMSettings.imagePromptProfileId` present (`typeof !== 'undefined'` — so an explicit `null` counts) ⇒ 400** `Invalid settings: <keys joined ', '> was\|were replaced by conciergeSettings` | the updated settings row (seeded into the cache) | #76 (`app/api/v1/settings/chat/route.ts`). |
| `POST /api/v1/chats/[id]/messages/[messageId]?action=retry-uncensored[&stream=1]` | NONE | no body. 404 `Chat`/`Message`; 400 `Only assistant messages can be retried` (non-ASSISTANT), 400 `Staff and system messages cannot be regenerated` (systemSender); **409 `{ error: 'locked' }` / `{ error: 'no-understudy' }`** | `stream=1`: the swipe SSE narration (`streamSwipeRegeneration` — identical frames to `?action=swipe&stream=1`); else **201 `{ message: newSwipe }`**; 500 `{ error }` on failure. The new swipe's `routeTrail` ends `via: 'concierge'` (`composeRetryRouteTrail(prior, understudy, 'connection')`). Chat state never changed. | #77 NEW. |
| `POST /api/v1/chats/[id]?action=retry-image-uncensored` | NONE | body `{ toolMessageId: string(min 1) }` \| `{ kind: 'background' }` (zod union; else 400 `Expected { toolMessageId } or { kind: "background" }`). Picture: 404 `Tool message` (missing or not TOOL); 400 `Only generate_image pictures can be retried uncensored`; 409 `locked`/`no-understudy`; 502 `{ error: result.message \|\| 'The uncensored desk could not produce the picture', code: result.error }` | picture: **200 `{ toolMessageId, images: GeneratedImage[], routeTrail }`** — a NEW TOOL row filed at `original.createdAt + 1 ms` (sits beside the original), plus a Concierge `refusal-rerouted` bubble (purpose `tool`) iff the original trail had a `refused` row; background: delegates to `handleRegenerateBackground(…, { forceUncensored: true })` — same body as `?action=regenerate-background` (queued / already-in-progress message) | #77 NEW. Client reads only `ok`/status/`error`. |
| message `routeTrail` (all rows incl. TOOL + Lantern/Aurora bubbles) | `MessageDto.routeTrail` | — | `RouteAttempt { profileId, profileName, provider, modelName, via, outcome, trigger?, evidence?: 'typed-error'\|'provider-code'\|'finish-reason'\|'message-pattern'\|'inferred', profileKind?: 'connection'\|'image' (absent = connection), detail? (≤200) }` | #73 widening. |
| Concierge + Lantern system messages | transcript rows | — | `systemSender`/`systemKind` as below | #73–#77 |

`conciergeSettings` zod (`lib/schemas/settings.types.ts` at `3b463d6b1`, verbatim):

```ts
export const DangerousContentDisplayModeEnum = z.enum(['SHOW', 'BLUR', 'COLLAPSE']);
export const ConciergeNewChatStateEnum = z.enum(['moderated', 'unmoderated']);
export const ConciergeDisplaySettingsSchema = z.object({
  mode: DangerousContentDisplayModeEnum.default('SHOW'),
  showWarningBadges: z.boolean().default(true),
});
export const ConciergePreScreenSettingsSchema = z.object({
  enabled: z.boolean().default(false),
  threshold: z.number().min(0).max(1).default(0.7),
  scanTextChat: z.boolean().default(true),
  scanImagePrompts: z.boolean().default(true),
  scanImageGeneration: z.boolean().default(false),
  customClassificationPrompt: z.string().nullable().optional(),
  summaryClassification: z.boolean().default(false),
});
export const ConciergeSettingsSchema = z.object({
  enabled: z.boolean().default(true),
  uncensoredTextProfileId: UUIDSchema.nullable().optional(),
  uncensoredImageProfileId: UUIDSchema.nullable().optional(),
  uncensoredVisionProfileId: UUIDSchema.nullable().optional(),
  imagePromptProfileId: UUIDSchema.nullable().optional(),
  autoSwitchAfterRefusals: z.number().int().min(0).max(10).default(2),
  newChatsStartAs: ConciergeNewChatStateEnum.default('moderated'),
  display: ConciergeDisplaySettingsSchema.default({ mode: 'SHOW', showWarningBadges: true }),
  preScreen: ConciergePreScreenSettingsSchema.default({
    enabled: false, threshold: 0.7, scanTextChat: true, scanImagePrompts: true,
    scanImageGeneration: false, summaryClassification: false,
  }),
});
// ChatSettingsSchema: conciergeSettings: ConciergeSettingsSchema.default({ enabled: true,
//   autoSwitchAfterRefusals: 2, newChatsStartAs: 'moderated', display: {…}, preScreen: {…} })
```
(Removed: `DangerousContentModeEnum`, `DangerousContentSettingsSchema`,
`ChatSettingsSchema.uncensoredImageDescriptionProfileId`,
`CheapLLMSettingsSchema.imagePromptProfileId`.) Client default object =
`resolver.service.ts` `DEFAULT_CONCIERGE_SETTINGS` (all profile ids `null`,
`customClassificationPrompt: null`, everything else as the schema defaults).

System messages the client renders (from the writers at `ce2f1dabf`; all `role: 'ASSISTANT'`,
`participantId: null`):

| writer / kind (internal) | systemSender | persisted systemKind | collapsed-bar label | importance |
|---|---|---|---|---|
| classifier verdict (`postConciergeDangerAnnouncement`) | `concierge` | `danger` | (danger kind label) | high |
| `set-moderated` | `concierge` | `danger` | same | high |
| `set-unmoderated` | `concierge` | `danger` | same | high |
| `set-locked` | `concierge` | `danger` | same | high |
| `auto-unmoderated` (refusal ledger) | `concierge` | `danger` | same | high |
| `refusal-rerouted` | `concierge` | `refusal` | `provider refusal` | high |
| `refusal-no-understudy` | `concierge` | `refusal` | `provider refusal` | high |
| `refusal-not-permitted` (reason `locked`) | `concierge` | `refusal` | `provider refusal` | high |
| Lantern `background-refused` (`postLanternRefusalNotification`; may carry `routeTrail`) | `lantern` | `background-refused` | `backdrop refused` | high |

Retired kinds (`manual-flagged`, `manual-safe`, `manual-resumed`, `manual-vouched`,
`manual-uncensored`, `auto-flagged-refusals`) survive in old transcripts as ordinary `danger`
rows. Voiced content (verbatim, for the e2e phrase checks): `set-moderated` "By the operator's
own hand, the conversation is Moderated once more. …His ledger of refusals is wiped clean.";
`set-unmoderated` "By the operator's own hand, the Concierge has been sent away and the
uncensored door stands open. …"; `set-locked` "The operator has locked the present company to the
house's usual desks. …"; `auto-unmoderated` "[Twice now|Once…] the house's regular staff have
declined this conversation on grounds of propriety — most recently <P M>. The Concierge has taken
the liberty of moving the whole affair to the uncensored desk; you may set it back to Moderated
from the sidebar whenever you wish."; refusal bubbles "The Concierge regrets to report that the
house's usual painter (<P> <M>) declined the commission for a picture on grounds of propriety;
he has taken it across the street to <profile>, who were happy to oblige." (and the
`-no-understudy`/`-not-permitted` variants; `text` purpose says "the house's usual correspondent"
/ "the request for a reply"); Lantern "The Lantern's usual painter (<P> <M>) would not take the
scene — called it improper and downed brushes. The backdrop stays as it was." Each also has an
`opaqueContent` twin.

---

## D. Traps

1. **Commit prose vs hunks.**
   - #75's message says "GET returns conciergeState / conciergeSetBy / conciergeReason /
     conciergeRefusalCount" — correct; the ORDER's working names (`conciergeStateSetBy`,
     `conciergeStateReason`, `moderationRefusalCount`) are WRONG for the wire.
   - #77 "Try uncensored … in the refusal bubble": the hunks put a button on the **Lantern's
     `background-refused` staff bubble only** — the Concierge's own `refusal` bubbles get NO
     button (the retry for a refused picture lives on the TOOL row).
   - #77 "hidden on Locked" is the ONLY gate — not on-duty, not "desk configured"; the server's
     409 `no-understudy` is how a missing desk surfaces. Off duty, the buttons still show (the
     server's `resolveConfiguredConciergeDesk` ignores duty by design).
   - #74's settings number input is dead code after #76 (whole file deleted); port only
     RefusalsCard's version (label says "Unmoderated", help text differs from #74's).
   - #76 "Salon display comes from the chat's resolved policy (no badges on Unmoderated chats)"
     — v5 has no badges to hide (D.3).
   - `lib/chat-utils.ts` is in the order's scope but only carries two new pass-through keys; v5
     has no transform layer (cards bind DTOs), so the work lands on the DTOs instead.
2. **`describeConciergeState` argument order moved** (provenance inserted 2nd). v5 has two
   2-arg call sites (`conversation-header.ts:172-174`, `concierge-mark.ts:119-121`) — a
   `string[]` passed as the 2nd arg would type-error in TS only if the note type is strict;
   make the provenance param a named object type so a stale call can't compile.
3. **No v5 counterpart:** the message danger-flag UI (badges, BLUR/COLLAPSE, "Not Dangerous",
   `dangerFlags` projection, `override-danger-flag` verb) — CONFIRMED absent; so `#76`'s
   `conciergeDisplay` and `#77`'s standing-flag lift have NO consumer. The DisplayCard settings
   are therefore write-only in v5 (as the old `displayMode` already was). Also absent: the
   Foundry `subsystem-defaults` description source (`useSubsystemInfo`), `/foundry/concierge`
   redirect, `ConciergeOffDutyHint`, continuation `initialConciergeState`, a second folded
   ToolMessage site in message-row (v5 has one).
4. **Behaviour inversions that existing v5 e2e pin the old way:** `shouldShowDangerStyling` is
   now Unmoderated **either provenance** (v5's `salon-danger-avatar-flow` asserts operator
   Uncensored is NOT ringed — inverts); quick-hide now hides operator-Unmoderated too (was
   already true for Uncensored — no change) but NOT Locked; participant-card tint follows the same
   predicate.
5. **Retired-key 400 hits OTHER v5 writers.** v4 rejects a PUT whose `cheapLLMSettings` merely
   CONTAINS an `imagePromptProfileId` key (`typeof !== 'undefined'`, so `null` counts).
   `screens/settings/providers/cheap-llm-card.ts:256-261` PUTs `{ ...this.cheap(), ...patch }`
   where `cheap()` spreads the whole GET bag and `CheapLLMSettings` (chat-settings.types :282)
   still declares the key — if the v5 server's GET still returns it (e.g. a stored JSON bag that
   was not stripped by the migration, or a default carrying `imagePromptProfileId: null`), EVERY
   cheap-LLM save 400s. Same for `dangerous-content-settings.ts:440`. The server port must strip
   it on read (v4's zod strips unknown keys on read); the SPA must drop the key from its type and
   never spread an unknown bag blindly. Likewise v5's `image-description-settings.ts:151` sends
   `uncensoredImageDescriptionProfileId` ⇒ 400.
6. **Whole-object PUT.** `conciergeSettings` is replaced wholesale — the SPA must deep-merge
   `display`/`preScreen` over defaults+current before sending (v4 `handleConciergeUpdate`). v5's
   per-card `saving` departure (`ChatSettingsCard`) means two cards can race: v4 guards with
   `settingsRef.current`; v5 must merge over the LATEST cache value at send time, not a
   render-time computed (a per-keystroke number/textarea save makes this real).
7. **Query keys.** v5 has TWO chat-settings keys: `['chatSettings']` (`chatSettingsKeys.all`;
   Salon, new-chat-form, cheap-llm, appearance) and `['chat-settings']` (roleplay-templates card);
   `workspace/core/tab-refetch.ts:92` refetches both. The sidebar's on-duty read and the New Chat
   seed must use `chatSettingsKeys.all` so a Concierge-tab save (which `setQueryData`s that key)
   flips the disabled select live — v4 relies on the shared `queryKeys.settings.chat`. After a
   per-chat flip v4 only calls the parent `fetchChat` (no list invalidation) — v5's existing
   `chatUpdated` emit → chat refetch matches; the realtime invalidation subsystem may already
   refresh lists. After a picture retry v4 `fetchChat()`s; after a backdrop retry it
   `notifyQueueChange()` + starts the background poll; after a text retry the regeneration
   controller's refetch + `selectSwipeVariant(newSwipeId)` (v5's `onRegenerate` already does the
   synchronous seed — reuse it verbatim for the retry, including the bug-(b) handling).
8. **Streaming transport.** v4's text retry is a URL swap on the SSE swipe. v5 has no URL: it
   dispatches `messageSwipe {stream:true}` and filters `swipeProgress` frames by `messageId`.
   The retry needs a server verb that emits the SAME frame family keyed by the target message
   id, or `RegenerationController.regenerate` grows a `request` option. 409 mapping: v4 reads
   `info.error` ('locked'/'no-understudy'); in v5 that arrives as `CoreDispatchError.message`
   with `kind` = the 409 kind (v5's `ErrorKind` has no explicit `'conflict'` member — check what
   the server maps 409 to) — map by message, and keep the fallthrough to the raw message.
9. **Angular select re-write.** `chat-section.ts`'s `afterRenderEffect` re-applies the derived
   value after each render (P4.D141 idiom). With `[disabled]` now also bound to on-duty, keep the
   effect reading `conciergeSaving()` AND the on-duty signal so a disabled→enabled flip re-syncs.
   The New Chat form's one-shot `newChatsStartAs` seed must be a guarded effect (v4's
   `conciergeSeededRef`) — a naive `effect()` on the settings query would re-seed on every
   project/cast reload and overwrite the user's pick.
10. **Memo identity.** v4 builds `conciergeRetry` with `useMemo` so rows don't re-render; in
    Angular pass it as a `computed` returning the SAME object unless `locked`/chat-presence
    changes (OnPush inputs compare by reference) — and the Lantern/tool buttons key off its
    presence, not a separate boolean.
11. **Portals/overlays.** None of the new controls are portaled. The only tooltip is the action
    bar's `qt-tooltip` (existing idiom). The info toast in `retryPicture` fires BEFORE the POST.
12. **Two `ConciergeState` homes in v5** (`core/core-contract.ts:109` and
    `chat/concierge-state.ts:70`) — both must move together; a green `ng build` does not prove
    the contract one moved (`spa-tsc-does-not-typecheck-app-sources`). Also `ConciergeTone`
    `'info'` removal ripples to `conciergeToneSuffix`'s return type and the header's
    `badgeSuffixClass` comment.
13. **`-info` tone's remaining v5 users** (concierge-only, all to delete):
    `concierge-state-presentation.ts:39,103,121`; CSS `_chat.css:3062-3065, 3084-3087`; specs
    `concierge-mark.spec.ts:93,110,185,190`, `conversation-header.spec.ts:274,302`,
    `chat-section.spec.ts:556`, `concierge-state-presentation.spec.ts:124`,
    `e2e/concierge-marks-flow.spec.ts:65`. NOT concierge (leave alone): `qt-text-info` in
    `memory/memory-card.ts:42`, `chat/llm-inspector-entry.ts:21,31,32,34`,
    `images/provider-icon.ts:12-13`. Removing the two CSS rules is safe for `check-qt-classes`
    only after every reference above is gone (the guard flags refs without rules).
14. **`inferKindFromContent` false-positive risk:** the new concierge inference
    `c.includes('on grounds of propriety')` would also match `set-moderated` and
    `auto-unmoderated` content — harmless in practice because those rows carry
    `systemKind: 'danger'`; transcribe verbatim anyway (v5 is v4's twin).
15. **List tooltip lacks the count.** Only the header pill and the sidebar helper pass
    `refusalCount`; list marks never do (payloads don't carry it). Don't "fix" this.
16. **`getConciergeProvenance` defaults to `'operator'`** for a non-Moderated chat with a
    missing setBy — a server that projects `conciergeSetBy: null` on Unmoderated would render the
    operator sentence, not crash.
17. **Settings `section` in hosted mode.** v5's `Settings` accepts but does not thread `section`
    when hosted as a workspace tab (settings.ts :98-103, mirroring v4). The off-duty hint's
    `&section=on-duty` and the image-description link's `&section=uncensored-desk` therefore only
    force-open in routed mode — On Duty and the Desk are `defaultOpen` anyway, so no visible gap;
    the scroll-into-view (v4 test "force-opens and scrolls…") happens only in routed mode.
18. **Help vendor count.** `dangerous-content.md` deleted + `the-concierge.md` added: 129 stays
    129, but the slug changes; `help-categories.ts`, both fixtures and the help-tree lane move
    together (`a-vendored-count-is-hard-coded-in-several-crates`).

---

## E. Open questions

1. **v5 has no danger-flag UI.** Rule: do the DisplayCard settings ship as write-only (as today),
   or does this round finally port the flag badges/blur/"Not Dangerous" (a whole new vertical:
   `dangerFlags` projection + `override-danger-flag` verb + DangerFlagBadge)? The survey assumes
   OUT of scope and records a named divergence.
2. **Text retry transport:** a new dispatch verb (`messageRetryUncensored { messageId, stream }`
   emitting `swipeProgress` frames keyed by the target id) vs. a flag on `messageSwipe`. Server
   lane's call; the SPA only needs the frames keyed by the target message id + a 201-shaped
   `{ message }` answer.
3. **Picture retry verb** (`chatRetryImageUncensored { chatId, toolMessageId } | { chatId, kind:
   'background' }`) — how does the v5 tri-state/`request_envelope` convention want the union
   decoded, and does the 502 `{error, code}` map to a `CoreError.code`?
4. **Does a PUT `conciergeState` get refused while OFF DUTY?** v4's POST ignores it; the PUT path
   (`processChatUpdates` → `applyConciergeFlip`) was not surveyed here — the SPA disables the
   select, but the e2e must know whether a direct dispatch 400s, no-ops, or applies.
5. **What v5 error `kind` does a 409 carry** (the retry refusals)? `ErrorKind` lists no
   `conflict`; confirm the server's mapping before writing the `describeRetryRefusal` arm.
6. **Tab intro text:** v4 reads `info.description` from the subsystem registry (theme-overridable).
   v5 hard-codes tab intros — hard-code **`Who gets asked when the usual providers refuse, and
   how flagged content is shown`** (the default), or is a theme override path owed?
7. **`newChatsStartAs` seed vs the existing `defaultRoleplayTemplateId` seed** in
   `new-chat.state.ts` — same load, same "only once / until touched" semantics? (v4: concierge
   seeds ONCE per hook life; the template re-seeds until touched — different rules.)
8. **Does v5 need `/foundry/concierge`?** No Foundry routes exist in v5; confirm it stays absent.
9. **Should the Settings shell's `section` be threaded when hosted**, given the new tab's
   `?section=` deep links from the Salon (off-duty hint) and the Chat tab (image description)?
   v4 does not; keeping parity is the default.
