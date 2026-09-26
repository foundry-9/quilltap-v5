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
  resolveDangerousContentSettings,
} from '@/lib/services/dangerous-content/resolver.service'
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
import {
  DangerousContentSettingsSchema,
  type DangerousContentSettings,
} from '@/lib/schemas/settings.types'

// A fully-materialized (Zod-shaped) settings object — what the repository hands
// the resolver in production. Built from v4's REAL schema defaults
// (`parse({})`) so it is the right shape at ANY pin: since `49059fb14` (#74,
// P4.D225) that shape carries `autoSwitchAfterRefusals: 2`, which a hand-written
// literal silently lacked. Optional profile ids are kept ABSENT (never explicit
// null) so the 'global' passthrough round-trips byte-for-byte through the Rust
// typed struct (the null-vs-absent optional is a documented corpus constraint).
function settings(mode: string, extra: Partial<DangerousContentSettings> = {}): DangerousContentSettings {
  return {
    ...DangerousContentSettingsSchema.parse({}),
    mode: mode as DangerousContentSettings['mode'],
    ...extra,
  }
}

type ChatView = Record<string, unknown>

// --- resolver matrix ---
// The per-chat arms are keyed on `conciergeMode` (v4 `4d370a90f`). The rows that
// still name the legacy pair PROVE it is ignored: v4's own resolver test
// "ignores the legacy conciergeOverride column".
const resolveCases: Array<{ id: string; global: DangerousContentSettings | null; chat: ChatView | null }> = [
  { id: 'no-settings-no-chat', global: null, chat: null },
  { id: 'global-auto-route-no-chat', global: settings('AUTO_ROUTE'), chat: null },
  { id: 'global-detect-only', global: settings('DETECT_ONLY'), chat: { chatType: 'salon', conciergeMode: null } },
  { id: 'global-off', global: settings('OFF'), chat: { chatType: 'salon' } },
  { id: 'help-exempt', global: settings('AUTO_ROUTE'), chat: { chatType: 'help', conciergeMode: 'locked' } },
  { id: 'brahma-exempt', global: settings('AUTO_ROUTE'), chat: { chatType: 'brahma' } },
  { id: 'default-no-global-plain-chat', global: null, chat: { chatType: 'salon' } },
  { id: 'global-with-uncensored', global: settings('AUTO_ROUTE', { uncensoredTextProfileId: 'prof-unc-1' }), chat: { chatType: 'salon' } },
  { id: 'global-with-custom-prompt', global: settings('DETECT_ONLY', { customClassificationPrompt: 'Also flag squick.' }), chat: null },
  // --- per-chat Locked (v4 resolver.test.ts `per-chat Locked state`) ---
  { id: 'locked-collapses', global: settings('AUTO_ROUTE'), chat: { chatType: 'salon', conciergeMode: 'locked' } },
  { id: 'locked-no-global', global: null, chat: { conciergeMode: 'locked' } },
  { id: 'locked-over-global-auto-route-with-ids', global: settings('AUTO_ROUTE', { uncensoredTextProfileId: 'prof-unc-1', autoSwitchAfterRefusals: 5 }), chat: { chatType: 'salon', conciergeMode: 'locked', conciergeModeSetBy: 'operator', conciergeModeReason: 'manual' } },
  { id: 'moderated-respects-global', global: settings('DETECT_ONLY'), chat: { chatType: 'salon', conciergeMode: 'moderated' } },
  // The legacy pair is ignored: an `OFF` override is NOT Locked any more, an
  // `UNCENSORED` one is NOT Unmoderated, a dangerous label is NOT Unmoderated.
  { id: 'legacy-off-ignored', global: settings('AUTO_ROUTE'), chat: { chatType: 'salon', conciergeOverride: 'OFF' } },
  { id: 'legacy-uncensored-ignored', global: settings('OFF'), chat: { chatType: 'salon', conciergeOverride: 'UNCENSORED', isDangerousChat: true } },
  // --- per-chat Unmoderated: AUTO_ROUTE forced even under a global OFF ---
  {
    id: 'unmoderated-forces-auto-route-under-global-off',
    global: settings('OFF', {
      scanImageGeneration: true,
      uncensoredTextProfileId: '11111111-1111-4111-8111-111111111111',
      uncensoredImageProfileId: '22222222-2222-4222-8222-222222222222',
    }),
    chat: { chatType: 'salon', conciergeMode: 'unmoderated', conciergeModeSetBy: 'operator', conciergeModeReason: 'manual' },
  },
  {
    id: 'unmoderated-by-concierge-over-global-auto-route',
    global: settings('AUTO_ROUTE', { uncensoredTextProfileId: 'prof-unc-1' }),
    chat: { chatType: 'salon', conciergeMode: 'unmoderated', conciergeModeSetBy: 'concierge', conciergeModeReason: 'refusals' },
  },
  // No global settings at all: v4 spreads DEFAULT_DANGEROUS_CONTENT_SETTINGS.
  { id: 'unmoderated-no-global', global: null, chat: { conciergeMode: 'unmoderated' } },
  // The payload key (a server-derived chat) reads the same as the column.
  { id: 'unmoderated-by-payload-key', global: settings('DETECT_ONLY'), chat: { conciergeState: 'unmoderated' } },
  // Branch order: exempt beats Unmoderated and Locked (v4's own test pins the
  // first: "moderation-exempt chat types win over the Unmoderated state").
  {
    id: 'brahma-exempt-beats-unmoderated',
    global: settings('AUTO_ROUTE', { uncensoredTextProfileId: 'prof-unc-1' }),
    chat: { chatType: 'brahma', conciergeMode: 'unmoderated' },
  },
  {
    id: 'help-exempt-beats-unmoderated',
    global: settings('DETECT_ONLY'),
    chat: { chatType: 'help', conciergeMode: 'unmoderated' },
  },
  // An unknown stored value reads as Moderated.
  { id: 'unknown-mode-reads-moderated', global: settings('DETECT_ONLY'), chat: { conciergeMode: 'vouched' } },
]

for (const c of resolveCases) {
  const globalSettings = c.global ? ({ dangerousContentSettings: c.global } as any) : null
  const r = resolveDangerousContentSettings(globalSettings, c.chat as any)
  process.stdout.write(
    JSON.stringify({ kind: 'resolve', id: c.id, global: c.global, chat: c.chat, settings: r.settings, source: r.source }) + '\n'
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
