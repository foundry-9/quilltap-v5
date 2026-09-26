/**
 * Oracle case (W4.2 dangerous-content): the mode resolver + the per-chat
 * Concierge truth table. Pure functions — exact equality.
 *
 * P4.D226 (v4 `4d370a90f`, #75 — the three states) REWROTE both halves: the
 * state lives in `chats.conciergeMode` (+ provenance `conciergeModeSetBy` /
 * `conciergeModeReason`), the legacy pair (`conciergeOverride`,
 * `isDangerousChat`) is IGNORED by every predicate, and the resolver's two
 * per-chat sources are `chat-locked` / `chat-unmoderated`. The override rows
 * now carry v4's own `chat-override.test.ts` 3x2 TABLE (state x provenance)
 * plus the payload-key reads (`conciergeState` / `conciergeSetBy` /
 * `conciergeReason`), and ask every question the module exports:
 * `getConciergeProvenance`, `getConciergeReason`, `mayFailOver` and its
 * state-only twin besides the four the table had. Two new kinds:
 * `derive` (`deriveConciergeModeFromLegacy`, v4's eight-row `it.each`) and
 * `withLegacy` (`withConciergeModeFromLegacy`, the object it returns in key
 * order — `toBe(chat)` identity rendered as `identical: true`). `stateRoute`
 * drives both state-only twins on each of the THREE literal states, and a
 * `states` row records `CONCIERGE_STATES` in control order.
 *
 * P4.D143 (v4 `c43d3b1b4`) adds the state-only twin
 * `conciergeStateUsesUncensoredRoute` — THE one place naming the uncensored row,
 * which `shouldUseUncensoredRoute` now delegates to. Every override row carries
 * it (driven through `getConciergeState(chat)`, v4's own `it.each(TABLE)`
 * agreement claim), and a `stateRoute` row drives it DIRECTLY on each of the
 * four literal states — the arm no chat-shaped case can reach.
 *
 * P4.D141 (v4 `60e3c4a0a`) widened both halves to the four-state control: the
 * override table now asks all THREE purpose-named questions over the full
 * stored-field 2x2 (v4's own `chat-override.test.ts` TABLE, row for row), and
 * the resolver grows the `chat-uncensored` arm alongside the renamed
 * `chat-vouched`.
 *
 * Drives the REAL exports from v4:
 *   lib/services/dangerous-content/resolver.service.ts:
 *     resolveDangerousContentSettings
 *   lib/services/dangerous-content/chat-override.ts:
 *     getConciergeState, conciergeStateUsesUncensoredRoute,
 *     shouldUseUncensoredRoute, shouldShowDangerStyling, isClassifierOnDuty
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/danger-resolver.ts \
 *     > /tmp/oracle-danger-resolver.ndjson
 */

import {
  DEFAULT_AUTO_SWITCH_AFTER_REFUSALS,
  DEFAULT_CONCIERGE_SETTINGS,
  readConciergeSettings,
  resolveConciergeSettings,
} from '@/lib/services/dangerous-content/resolver.service'
import {
  mapLegacyConciergeSettings,
  withConciergeSettingsFromLegacy,
} from '@/lib/services/dangerous-content/legacy-concierge-settings'
import {
  CONCIERGE_STATES,
  getConciergeState,
  getConciergeProvenance,
  getConciergeReason,
  conciergeStateUsesUncensoredRoute,
  conciergeStateMayFailOver,
  shouldUseUncensoredRoute,
  shouldShowDangerStyling,
  isClassifierOnDuty,
  mayFailOver,
  deriveConciergeModeFromLegacy,
  withConciergeModeFromLegacy,
} from '@/lib/services/dangerous-content/chat-override'
import type { ConciergeSettings } from '@/lib/schemas/settings.types'
// P4.D228 (v4 `ce2f1dabf`, #77): the configured desk. Imported as a namespace
// so the case still loads at a pin that predates it (the rows are simply
// absent there, and the Rust shape guard says so).
import * as resolverNs from '@/lib/services/dangerous-content/resolver.service'

type ChatView = Record<string, unknown>

// --- the defaults (v4 resolver.test.ts `DEFAULT_CONCIERGE_SETTINGS`) ---
process.stdout.write(
  JSON.stringify({
    kind: 'defaults',
    id: 'default-concierge-settings',
    settings: DEFAULT_CONCIERGE_SETTINGS,
    autoSwitch: DEFAULT_AUTO_SWITCH_AFTER_REFUSALS,
  }) + '\n'
)

// v4 `resolver.test.ts`'s own builders, verbatim: a fully-populated global
// object with the four desk ids, a non-default auto-switch and an opted-in
// pre-screen, over which each case lays its overrides (nested one level).
const TEXT_ID = '11111111-1111-4111-8111-111111111111'
const IMAGE_ID = '22222222-2222-4222-8222-222222222222'
const VISION_ID = '33333333-3333-4333-8333-333333333333'
const PROMPT_ID = '44444444-4444-4444-8444-444444444444'

function concierge(overrides: Partial<ConciergeSettings> = {}): ConciergeSettings {
  return {
    ...DEFAULT_CONCIERGE_SETTINGS,
    uncensoredTextProfileId: TEXT_ID,
    uncensoredImageProfileId: IMAGE_ID,
    uncensoredVisionProfileId: VISION_ID,
    imagePromptProfileId: PROMPT_ID,
    autoSwitchAfterRefusals: 3,
    ...overrides,
    display: { ...DEFAULT_CONCIERGE_SETTINGS.display, ...(overrides.display ?? {}) },
    preScreen: {
      ...DEFAULT_CONCIERGE_SETTINGS.preScreen,
      enabled: true,
      threshold: 0.55,
      scanTextChat: true,
      scanImagePrompts: false,
      scanImageGeneration: true,
      customClassificationPrompt: 'Be strict about gore.',
      summaryClassification: true,
      ...(overrides.preScreen ?? {}),
    },
  }
}
const global = (settings: unknown) => ({ conciergeSettings: settings })

const moderated = { conciergeMode: 'moderated', chatType: 'salon' }
const unmoderated = { conciergeMode: 'unmoderated', chatType: 'salon' }
const locked = { conciergeMode: 'locked', chatType: 'salon' }

// --- readConciergeSettings (v4 resolver.test.ts + the merge's edges) ---
const readCases: Array<{ id: string; global: unknown }> = [
  { id: 'no-settings-row', global: null },
  { id: 'no-settings-undefined', global: undefined },
  { id: 'concierge-settings-missing', global: global(undefined) },
  { id: 'concierge-settings-null', global: global(null) },
  { id: 'fills-nested-gaps', global: global({ enabled: false, preScreen: { enabled: true }, display: { mode: 'BLUR' } }) },
  // `?? {}` — a stored `null` nested object spreads nothing.
  { id: 'nested-null-spreads-nothing', global: global({ display: null, preScreen: null, autoSwitchAfterRefusals: 0 }) },
  // The fresh DDL's `.default()` literal: no ids, no prompt.
  { id: 'ddl-default-literal', global: global({ enabled: true, autoSwitchAfterRefusals: 2, newChatsStartAs: 'moderated', display: { mode: 'SHOW', showWarningBadges: true }, preScreen: { enabled: false, threshold: 0.7, scanTextChat: true, scanImagePrompts: true, scanImageGeneration: false, summaryClassification: false } }) },
  { id: 'full-object', global: global(concierge({ newChatsStartAs: 'unmoderated' })) },
  { id: 'integral-threshold', global: global({ preScreen: { threshold: 1 } }) },
]
for (const c of readCases) {
  process.stdout.write(
    JSON.stringify({ kind: 'readSettings', id: c.id, global: c.global === undefined ? '<undefined>' : c.global, settings: readConciergeSettings(c.global as any) }) + '\n'
  )
}

// --- resolveConciergeSettings (v4 resolver.test.ts, every describe) ---
const resolveCases: Array<{ id: string; global: unknown; chat: ChatView | null | undefined }> = [
  // enabled: false (off duty) — "allows nothing anywhere, with or without a chat"
  { id: 'off-duty-no-chat', global: global(concierge({ enabled: false })), chat: undefined },
  { id: 'off-duty-moderated', global: global(concierge({ enabled: false })), chat: moderated },
  { id: 'off-duty-unmoderated', global: global(concierge({ enabled: false })), chat: unmoderated },
  { id: 'off-duty-locked', global: global(concierge({ enabled: false })), chat: locked },
  // "still reports the state and newChatsStartAs"
  { id: 'off-duty-reports-state', global: global(concierge({ enabled: false, newChatsStartAs: 'unmoderated' })), chat: unmoderated },
  // Locked — "allows no failover, …, and empties the desk" / "keeps the global display settings"
  { id: 'locked', global: global(concierge()), chat: locked },
  { id: 'locked-keeps-display', global: global(concierge({ display: { mode: 'COLLAPSE', showWarningBadges: true } })), chat: locked },
  // Unmoderated — routes direct, the desk, hides badges
  { id: 'unmoderated', global: global(concierge()), chat: unmoderated },
  { id: 'unmoderated-blur', global: global(concierge({ display: { mode: 'BLUR', showWarningBadges: true } })), chat: unmoderated },
  // Moderated
  { id: 'moderated-global', global: global(concierge()), chat: moderated },
  { id: 'moderated-no-chat', global: global(concierge()), chat: undefined },
  { id: 'moderated-null-chat', global: global(concierge()), chat: null },
  { id: 'moderated-no-concierge-mode', global: global(concierge()), chat: { chatType: 'salon' } },
  { id: 'moderated-pre-screen-off-keeps-threshold-prompt', global: global(concierge({ preScreen: { enabled: false } as ConciergeSettings['preScreen'] })), chat: moderated },
  { id: 'moderated-summary-off', global: global(concierge({ preScreen: { summaryClassification: false } as ConciergeSettings['preScreen'] })), chat: moderated },
  { id: 'moderated-auto-switch-never', global: global(concierge({ autoSwitchAfterRefusals: 0 })), chat: moderated },
  // exempt chat types — every state
  ...['help', 'brahma'].flatMap((chatType) =>
    ['moderated', 'unmoderated', 'locked'].map((conciergeMode) => ({
      id: `exempt-${chatType}-${conciergeMode}`, global: global(concierge()), chat: { conciergeMode, chatType },
    }))),
  // exempt beats off duty (branch 1 before branch 2)
  { id: 'exempt-beats-off-duty', global: global(concierge({ enabled: false })), chat: { conciergeMode: 'locked', chatType: 'help' } },
  // missing conciergeSettings — the defaults with source "default"
  { id: 'missing-null', global: null, chat: moderated },
  { id: 'missing-undefined', global: undefined, chat: moderated },
  { id: 'missing-key', global: global(undefined), chat: moderated },
  { id: 'missing-unmoderated', global: null, chat: unmoderated },
  { id: 'missing-locked', global: null, chat: locked },
  // The stored DDL literal is truthy: source 'global' though nothing was chosen.
  { id: 'ddl-default-literal-reads-global', global: global({ enabled: true, autoSwitchAfterRefusals: 2, newChatsStartAs: 'moderated', display: { mode: 'SHOW', showWarningBadges: true }, preScreen: { enabled: false, threshold: 0.7, scanTextChat: true, scanImagePrompts: true, scanImageGeneration: false, summaryClassification: false } }), chat: moderated },
  // The legacy pair is ignored; the payload key reads like the column.
  { id: 'legacy-override-ignored', global: global(concierge()), chat: { chatType: 'salon', conciergeOverride: 'OFF', isDangerousChat: true } },
  { id: 'payload-key-unmoderated', global: global(concierge()), chat: { conciergeState: 'unmoderated' } },
  { id: 'unknown-mode-reads-moderated', global: global(concierge()), chat: { conciergeMode: 'vouched' } },
]
for (const c of resolveCases) {
  const policy = resolveConciergeSettings(c.global as any, c.chat as any)
  process.stdout.write(
    JSON.stringify({
      kind: 'resolve',
      id: c.id,
      global: c.global === undefined ? '<undefined>' : c.global,
      chat: c.chat === undefined ? '<undefined>' : c.chat,
      policy,
    }) + '\n'
  )
}

// --- mapLegacyConciergeSettings (v4 add-concierge-settings.test.ts, by name,
// plus every clamp / idOrNull / bool edge) — the migrated BYTES ---
const legacyCases: Array<{ id: string; sources: Record<string, unknown> }> = [
  { id: 'off-goes-off-duty', sources: { dangerousContentSettings: { mode: 'OFF' } } },
  { id: 'detect-only-on-duty', sources: { dangerousContentSettings: { mode: 'DETECT_ONLY' } } },
  { id: 'auto-route-on-duty', sources: { dangerousContentSettings: { mode: 'AUTO_ROUTE' } } },
  {
    id: 'carries-scans-threshold-prompt-desk-display-auto-switch',
    sources: {
      dangerousContentSettings: {
        mode: 'AUTO_ROUTE', threshold: 0.4, scanTextChat: false, scanImagePrompts: false, scanImageGeneration: true,
        uncensoredTextProfileId: TEXT_ID, uncensoredImageProfileId: IMAGE_ID, displayMode: 'COLLAPSE',
        showWarningBadges: false, customClassificationPrompt: 'Flag squick.', autoSwitchAfterRefusals: 5,
      },
      uncensoredImageDescriptionProfileId: VISION_ID,
      cheapLLMSettings: { strategy: 'PROVIDER_CHEAPEST', imagePromptProfileId: PROMPT_ID },
    },
  },
  { id: 'no-dangerous-content-settings', sources: {} },
  { id: 'null-dangerous-content-settings', sources: { dangerousContentSettings: null, cheapLLMSettings: null } },
  { id: 'off-with-unmoderated-chat', sources: { dangerousContentSettings: { mode: 'OFF' }, hasUnmoderatedChats: true } },
  { id: 'unknown-mode-is-off', sources: { dangerousContentSettings: { mode: 'SOMETIMES' } } },
  { id: 'lowercase-mode-is-off', sources: { dangerousContentSettings: { mode: 'auto_route' } } },
  { id: 'clamps-out-of-range', sources: { dangerousContentSettings: { mode: 'DETECT_ONLY', threshold: 1.5, autoSwitchAfterRefusals: 11, displayMode: 'HIDE' } } },
  { id: 'clamps-negative', sources: { dangerousContentSettings: { threshold: -0.1, autoSwitchAfterRefusals: -1 } } },
  { id: 'clamps-fractional-auto-switch', sources: { dangerousContentSettings: { autoSwitchAfterRefusals: 2.5, threshold: 1 } } },
  { id: 'wrong-types-fall-back', sources: { dangerousContentSettings: { threshold: '0.5', scanTextChat: 'yes', showWarningBadges: 0, autoSwitchAfterRefusals: '3', customClassificationPrompt: 7 } } },
  { id: 'empty-strings-are-null', sources: { dangerousContentSettings: { uncensoredTextProfileId: '', customClassificationPrompt: '' }, uncensoredImageDescriptionProfileId: '', cheapLLMSettings: { imagePromptProfileId: '' } } },
  { id: 'threshold-zero-and-one-survive', sources: { dangerousContentSettings: { threshold: 0, autoSwitchAfterRefusals: 0 } } },
  { id: 'has-unmoderated-must-be-true', sources: { dangerousContentSettings: { mode: 'OFF' }, hasUnmoderatedChats: 1 } },
]
for (const c of legacyCases) {
  process.stdout.write(
    JSON.stringify({ kind: 'legacyMap', id: c.id, sources: c.sources, migrated: mapLegacyConciergeSettings(c.sources as any) }) + '\n'
  )
}

// --- withConciergeSettingsFromLegacy: the returned record, in key order ---
const withSettingsCases: Array<{ id: string; settings: Record<string, unknown>; hasUnmoderatedChats: boolean }> = [
  { id: 'translates-when-absent', settings: { id: 's', dangerousContentSettings: { mode: 'AUTO_ROUTE' }, uncensoredImageDescriptionProfileId: VISION_ID }, hasUnmoderatedChats: false },
  { id: 'translates-a-null-in-place', settings: { id: 's', conciergeSettings: null, timezone: 'UTC', dangerousContentSettings: { mode: 'OFF' } }, hasUnmoderatedChats: true },
  { id: 'keeps-an-existing-object', settings: { id: 's', conciergeSettings: { enabled: false }, dangerousContentSettings: { mode: 'AUTO_ROUTE' } }, hasUnmoderatedChats: true },
  { id: 'keeps-an-empty-object', settings: { id: 's', conciergeSettings: {} }, hasUnmoderatedChats: false },
  { id: 'translates-an-empty-string', settings: { id: 's', conciergeSettings: '' }, hasUnmoderatedChats: false },
  { id: 'no-legacy-keys-at-all', settings: { id: 's' }, hasUnmoderatedChats: false },
]
for (const c of withSettingsCases) {
  const out = withConciergeSettingsFromLegacy(c.settings as any, c.hasUnmoderatedChats)
  process.stdout.write(
    JSON.stringify({ kind: 'withSettingsLegacy', id: c.id, settings: c.settings, hasUnmoderatedChats: c.hasUnmoderatedChats, out, identical: out === c.settings }) + '\n'
  )
}

// --- the Concierge truth table ---
// v4's own `chat-override.test.ts` 3x2 TABLE (state x provenance), in its order,
// plus the `getConciergeState` describe's rows (null / empty / NULL column /
// the legacy pair / the payload keys / column-beats-payload) and the edges a
// hydrated v5 row meets (absent keys; an unknown stored value; an unknown or
// NULL setBy on a non-Moderated row reading 'operator').
const overrideCases: Array<{ id: string; chat: ChatView | null | undefined }> = [
  { id: 'null-chat', chat: null },
  { id: 'undefined-chat', chat: undefined },
  { id: 'empty', chat: {} },
  { id: 'null-column', chat: { conciergeMode: null } },
  { id: 'legacy-pair-ignored', chat: { conciergeOverride: 'UNCENSORED', isDangerousChat: true } },
  { id: 'legacy-off-ignored', chat: { conciergeOverride: 'OFF', isDangerousChat: false } },
  { id: 'legacy-flagged-ignored', chat: { isDangerousChat: true } },
  // TABLE rows (`conciergeModeReason: setBy ? 'manual' : null`, as v4 builds them).
  { id: 'table-moderated-null', chat: { conciergeMode: 'moderated', conciergeModeSetBy: null, conciergeModeReason: null } },
  { id: 'table-moderated-stray-operator', chat: { conciergeMode: 'moderated', conciergeModeSetBy: 'operator', conciergeModeReason: 'manual' } },
  { id: 'table-unmoderated-operator', chat: { conciergeMode: 'unmoderated', conciergeModeSetBy: 'operator', conciergeModeReason: 'manual' } },
  { id: 'table-unmoderated-concierge', chat: { conciergeMode: 'unmoderated', conciergeModeSetBy: 'concierge', conciergeModeReason: 'manual' } },
  { id: 'table-locked-operator', chat: { conciergeMode: 'locked', conciergeModeSetBy: 'operator', conciergeModeReason: 'manual' } },
  { id: 'table-locked-null-setby', chat: { conciergeMode: 'locked', conciergeModeSetBy: null, conciergeModeReason: null } },
  // The payload keys (v4 "reads a server-derived payload (conciergeState) when the column is absent").
  { id: 'payload-locked', chat: { conciergeState: 'locked' } },
  { id: 'payload-unmoderated-concierge', chat: { conciergeState: 'unmoderated', conciergeSetBy: 'concierge' } },
  { id: 'payload-unmoderated-refusals', chat: { conciergeState: 'unmoderated', conciergeReason: 'refusals' } },
  // v4 "prefers the column over a derived payload value".
  { id: 'column-beats-payload', chat: { conciergeMode: 'moderated', conciergeState: 'locked' } },
  { id: 'column-setby-beats-payload', chat: { conciergeMode: 'unmoderated', conciergeModeSetBy: 'operator', conciergeSetBy: 'concierge', conciergeModeReason: 'classifier', conciergeReason: 'refusals' } },
  // A NULL column falls through to the payload key (JS `??`).
  { id: 'null-column-falls-to-payload', chat: { conciergeMode: null, conciergeState: 'unmoderated', conciergeModeSetBy: null, conciergeSetBy: 'concierge' } },
  // v4 getConciergeReason describe.
  { id: 'reason-null-for-moderated', chat: { conciergeMode: 'moderated', conciergeModeReason: 'refusals' } },
  { id: 'reason-stored', chat: { conciergeMode: 'unmoderated', conciergeModeReason: 'classifier' } },
  // Hydrated-row edges (a NULL nullable-optional is OMITTED on v5's read).
  { id: 'unmoderated-no-provenance-keys', chat: { conciergeMode: 'unmoderated' } },
  { id: 'unknown-setby-reads-operator', chat: { conciergeMode: 'unmoderated', conciergeModeSetBy: 'classifier', conciergeModeReason: 'refusals' } },
  { id: 'unknown-mode-reads-moderated', chat: { conciergeMode: 'flagged', conciergeModeSetBy: 'concierge' } },
  { id: 'every-reason', chat: { conciergeMode: 'locked', conciergeModeReason: 'migration' } },
]

for (const c of overrideCases) {
  const chat = c.chat as any
  process.stdout.write(
    JSON.stringify({
      kind: 'override',
      id: c.id,
      chat: c.chat === undefined ? '<undefined>' : c.chat,
      state: getConciergeState(chat),
      provenance: getConciergeProvenance(chat),
      reason: getConciergeReason(chat),
      uncensoredRoute: shouldUseUncensoredRoute(chat),
      stateUsesUncensoredRoute: conciergeStateUsesUncensoredRoute(getConciergeState(chat)),
      dangerStyling: shouldShowDangerStyling(chat),
      classifierOnDuty: isClassifierOnDuty(chat),
      mayFailOver: mayFailOver(chat),
      stateMayFailOver: conciergeStateMayFailOver(getConciergeState(chat)),
    }) + '\n'
  )
}

// --- the state-only twins, driven directly on each literal state ---
for (const state of ['moderated', 'unmoderated', 'locked'] as const) {
  process.stdout.write(
    JSON.stringify({
      kind: 'stateRoute',
      id: `state-${state}`,
      state,
      usesUncensoredRoute: conciergeStateUsesUncensoredRoute(state),
      mayFailOver: conciergeStateMayFailOver(state),
    }) + '\n'
  )
}

// --- CONCIERGE_STATES, in control order ---
process.stdout.write(JSON.stringify({ kind: 'states', id: 'concierge-states', states: CONCIERGE_STATES }) + '\n')

// --- deriveConciergeModeFromLegacy (v4's eight-row it.each, plus the absent-key
// hydrated shapes and a stray override string) ---
const deriveCases: Array<{ id: string; legacy: ChatView }> = [
  { id: 'uncensored-label-false', legacy: { conciergeOverride: 'UNCENSORED', isDangerousChat: false } },
  { id: 'uncensored-label-true', legacy: { conciergeOverride: 'UNCENSORED', isDangerousChat: true } },
  { id: 'off-label-true', legacy: { conciergeOverride: 'OFF', isDangerousChat: true } },
  { id: 'off-label-null', legacy: { conciergeOverride: 'OFF', isDangerousChat: null } },
  { id: 'null-override-label-true', legacy: { conciergeOverride: null, isDangerousChat: true } },
  { id: 'null-override-label-false', legacy: { conciergeOverride: null, isDangerousChat: false } },
  { id: 'null-override-label-null', legacy: { conciergeOverride: null, isDangerousChat: null } },
  { id: 'empty', legacy: {} },
  { id: 'label-true-no-override-key', legacy: { isDangerousChat: true } },
  { id: 'stray-override-string', legacy: { conciergeOverride: 'MAYBE', isDangerousChat: true } },
  { id: 'label-one-is-not-true', legacy: { isDangerousChat: 1 } },
]
for (const c of deriveCases) {
  process.stdout.write(
    JSON.stringify({ kind: 'derive', id: c.id, legacy: c.legacy, columns: deriveConciergeModeFromLegacy(c.legacy as any) }) + '\n'
  )
}

// --- withConciergeModeFromLegacy: the object it returns, in key order ---
const withCases: Array<{ id: string; chat: ChatView }> = [
  { id: 'derives-when-none', chat: { id: 'c', conciergeOverride: 'OFF', isDangerousChat: false } },
  { id: 'keeps-a-present-state', chat: { id: 'c', conciergeMode: 'moderated', conciergeOverride: 'UNCENSORED' } },
  { id: 'derives-over-a-present-null', chat: { id: 'c', conciergeMode: null, conciergeModeSetBy: null, title: 't', isDangerousChat: true } },
  { id: 'moderated-derivation-appends-nulls', chat: { id: 'c', title: 't' } },
  { id: 'keeps-locked-over-legacy', chat: { id: 'c', conciergeMode: 'locked', isDangerousChat: true } },
]
for (const c of withCases) {
  const out = withConciergeModeFromLegacy(c.chat as any)
  process.stdout.write(
    JSON.stringify({ kind: 'withLegacy', id: c.id, chat: c.chat, out, identical: out === c.chat }) + '\n'
  )
}

// --- resolveConfiguredConciergeDesk (P4.D228, v4 `ce2f1dabf` #77) ---
// The desk AS CONFIGURED, whatever the duty roster or the chat's state — only
// the operator's explicit "Try uncensored" reads it. There is no chat
// argument at all: Locked / exempt / off-duty cannot empty it.
const configuredDesk = (resolverNs as Record<string, unknown>).resolveConfiguredConciergeDesk as
  | ((g: unknown) => unknown)
  | undefined
if (configuredDesk) {
  const deskCases: Array<{ id: string; global: unknown }> = [
    { id: 'desk-no-row', global: null },
    { id: 'desk-undefined', global: undefined },
    { id: 'desk-settings-null', global: global(null) },
    { id: 'desk-on-duty', global: global(concierge({})) },
    { id: 'desk-off-duty-keeps-the-desk', global: global(concierge({ enabled: false })) },
    { id: 'desk-partial', global: global({ enabled: false, uncensoredImageProfileId: 'img-desk' }) },
  ]
  for (const c of deskCases) {
    process.stdout.write(
      JSON.stringify({ kind: 'configuredDesk', id: c.id, global: c.global === undefined ? '<undefined>' : c.global, desk: configuredDesk(c.global as any) }) + '\n'
    )
  }
}
