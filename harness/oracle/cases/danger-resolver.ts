/**
 * Oracle case (W4.2 dangerous-content): the mode resolver + the per-chat
 * override truth table. Pure functions — exact equality.
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
  getConciergeState,
  conciergeStateUsesUncensoredRoute,
  shouldUseUncensoredRoute,
  shouldShowDangerStyling,
  isClassifierOnDuty,
} from '@/lib/services/dangerous-content/chat-override'
import type { DangerousContentSettings } from '@/lib/schemas/settings.types'

// A fully-materialized (Zod-shaped) settings object. Optional profile ids are
// kept ABSENT (never explicit null) so the 'global' passthrough round-trips
// byte-for-byte through the Rust typed struct (the null-vs-absent optional is a
// documented corpus constraint).
function settings(mode: string, extra: Partial<DangerousContentSettings> = {}): DangerousContentSettings {
  return {
    mode: mode as DangerousContentSettings['mode'],
    threshold: 0.7,
    scanTextChat: true,
    scanImagePrompts: true,
    scanImageGeneration: false,
    displayMode: 'SHOW',
    showWarningBadges: true,
    ...extra,
  }
}

type ChatView = { conciergeOverride?: 'OFF' | 'UNCENSORED' | null; chatType?: string | null; isDangerousChat?: boolean | null }

// --- resolver matrix ---
const resolveCases: Array<{ id: string; global: DangerousContentSettings | null; chat: ChatView | null }> = [
  { id: 'no-settings-no-chat', global: null, chat: null },
  { id: 'global-auto-route-no-chat', global: settings('AUTO_ROUTE'), chat: null },
  { id: 'global-detect-only', global: settings('DETECT_ONLY'), chat: { chatType: 'salon', conciergeOverride: null } },
  { id: 'global-off', global: settings('OFF'), chat: { chatType: 'salon' } },
  { id: 'help-exempt', global: settings('AUTO_ROUTE'), chat: { chatType: 'help', conciergeOverride: 'OFF' } },
  { id: 'brahma-exempt', global: settings('AUTO_ROUTE'), chat: { chatType: 'brahma' } },
  { id: 'off-duty-collapses', global: settings('AUTO_ROUTE'), chat: { chatType: 'salon', conciergeOverride: 'OFF' } },
  { id: 'off-duty-no-global', global: null, chat: { conciergeOverride: 'OFF' } },
  { id: 'default-no-global-plain-chat', global: null, chat: { chatType: 'salon' } },
  { id: 'global-with-uncensored', global: settings('AUTO_ROUTE', { uncensoredTextProfileId: 'prof-unc-1' }), chat: { chatType: 'salon' } },
  { id: 'global-with-custom-prompt', global: settings('DETECT_ONLY', { customClassificationPrompt: 'Also flag squick.' }), chat: null },
  // --- P4.D141: the operator's Uncensored assertion (v4 `60e3c4a0a`) ---
  // The motivating regression: AUTO_ROUTE is forced even under a global OFF, so
  // asking for uncensored routing on one chat needs no global switch.
  {
    id: 'uncensored-forces-auto-route-under-global-off',
    global: settings('OFF', {
      scanImageGeneration: true,
      uncensoredTextProfileId: '11111111-1111-4111-8111-111111111111',
      uncensoredImageProfileId: '22222222-2222-4222-8222-222222222222',
    }),
    chat: { chatType: 'salon', conciergeOverride: 'UNCENSORED' },
  },
  {
    id: 'uncensored-over-global-auto-route',
    global: settings('AUTO_ROUTE', { uncensoredTextProfileId: 'prof-unc-1' }),
    chat: { chatType: 'salon', conciergeOverride: 'UNCENSORED', isDangerousChat: true },
  },
  // No global settings at all: v4 spreads DEFAULT_DANGEROUS_CONTENT_SETTINGS.
  { id: 'uncensored-no-global', global: null, chat: { conciergeOverride: 'UNCENSORED' } },
  // Branch order: exempt beats uncensored (v4's own test pins this).
  {
    id: 'brahma-exempt-beats-uncensored',
    global: settings('AUTO_ROUTE', { uncensoredTextProfileId: 'prof-unc-1' }),
    chat: { chatType: 'brahma', conciergeOverride: 'UNCENSORED' },
  },
  {
    id: 'help-exempt-beats-uncensored',
    global: settings('DETECT_ONLY'),
    chat: { chatType: 'help', conciergeOverride: 'UNCENSORED' },
  },
  // Vouched Safe still collapses, and the label underneath is preserved/ignored.
  { id: 'vouched-with-label-collapses', global: settings('AUTO_ROUTE'), chat: { chatType: 'salon', conciergeOverride: 'OFF', isDangerousChat: true } },
]

for (const c of resolveCases) {
  const globalSettings = c.global ? ({ dangerousContentSettings: c.global } as any) : null
  const r = resolveDangerousContentSettings(globalSettings, c.chat as any)
  process.stdout.write(
    JSON.stringify({ kind: 'resolve', id: c.id, global: c.global, chat: c.chat, settings: r.settings, source: r.source }) + '\n'
  )
}

// --- override truth table (the full four-state 2x2) ---
// Rows 3-9 are v4's own `chat-override.test.ts` TABLE, in its order: both
// stored fields across all four states, with the preserved `isDangerousChat`
// label in each operator position (the label must not leak into any predicate).
const overrideCases: Array<{ id: string; chat: ChatView | null }> = [
  { id: 'null-chat', chat: null },
  { id: 'empty', chat: {} },
  { id: 'monitored-explicit-false', chat: { conciergeOverride: null, isDangerousChat: false } },
  { id: 'monitored-label-null', chat: { conciergeOverride: null, isDangerousChat: null } },
  { id: 'flagged', chat: { conciergeOverride: null, isDangerousChat: true } },
  { id: 'vouched-label-false', chat: { conciergeOverride: 'OFF', isDangerousChat: false } },
  { id: 'vouched-label-true', chat: { conciergeOverride: 'OFF', isDangerousChat: true } },
  { id: 'uncensored-label-false', chat: { conciergeOverride: 'UNCENSORED', isDangerousChat: false } },
  { id: 'uncensored-label-true', chat: { conciergeOverride: 'UNCENSORED', isDangerousChat: true } },
  // Absent keys (the hydrated read OMITS a NULL nullable-optional).
  { id: 'flagged-no-override-key', chat: { isDangerousChat: true } },
  { id: 'danger-null-no-override-key', chat: { isDangerousChat: null } },
  { id: 'vouched-no-label-key', chat: { conciergeOverride: 'OFF' } },
  { id: 'uncensored-no-label-key', chat: { conciergeOverride: 'UNCENSORED' } },
]

for (const c of overrideCases) {
  process.stdout.write(
    JSON.stringify({
      kind: 'override',
      id: c.id,
      chat: c.chat,
      state: getConciergeState(c.chat as any),
      uncensoredRoute: shouldUseUncensoredRoute(c.chat as any),
      stateUsesUncensoredRoute: conciergeStateUsesUncensoredRoute(getConciergeState(c.chat as any)),
      dangerStyling: shouldShowDangerStyling(c.chat as any),
      classifierOnDuty: isClassifierOnDuty(c.chat as any),
    }) + '\n'
  )
}

// --- the state-only twin, driven directly on each literal state ---
// v4 `chat-override.test.ts`: "is the bottom row of the 2x2 and nothing else".
// No chat is involved, so this is the only place the four-state domain of
// `conciergeStateUsesUncensoredRoute` is exercised on its own.
for (const state of ['monitored', 'flagged', 'vouched', 'uncensored'] as const) {
  process.stdout.write(
    JSON.stringify({
      kind: 'stateRoute',
      id: `state-${state}`,
      state,
      usesUncensoredRoute: conciergeStateUsesUncensoredRoute(state),
    }) + '\n'
  )
}
