/**
 * Oracle case (W4.2 dangerous-content): the mode resolver + the per-chat
 * override truth table. Pure functions — exact equality.
 *
 * Drives the REAL exports from v4:
 *   lib/services/dangerous-content/resolver.service.ts:
 *     resolveDangerousContentSettings
 *   lib/services/dangerous-content/chat-override.ts:
 *     isConciergeOffDuty, getConciergeState, isChatActiveDangerous
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
  isConciergeOffDuty,
  getConciergeState,
  isChatActiveDangerous,
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

type ChatView = { conciergeOverride?: 'OFF' | null; chatType?: string | null; isDangerousChat?: boolean | null }

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
]

for (const c of resolveCases) {
  const globalSettings = c.global ? ({ dangerousContentSettings: c.global } as any) : null
  const r = resolveDangerousContentSettings(globalSettings, c.chat as any)
  process.stdout.write(
    JSON.stringify({ kind: 'resolve', id: c.id, global: c.global, chat: c.chat, settings: r.settings, source: r.source }) + '\n'
  )
}

// --- override truth table ---
const overrideCases: Array<{ id: string; chat: ChatView | null }> = [
  { id: 'null-chat', chat: null },
  { id: 'empty', chat: {} },
  { id: 'off-and-flagged', chat: { conciergeOverride: 'OFF', isDangerousChat: true } },
  { id: 'off-and-safe', chat: { conciergeOverride: 'OFF', isDangerousChat: false } },
  { id: 'flagged-on-duty', chat: { conciergeOverride: null, isDangerousChat: true } },
  { id: 'safe-on-duty', chat: { conciergeOverride: null, isDangerousChat: false } },
  { id: 'flagged-no-override', chat: { isDangerousChat: true } },
  { id: 'danger-null', chat: { isDangerousChat: null } },
]

for (const c of overrideCases) {
  process.stdout.write(
    JSON.stringify({
      kind: 'override',
      id: c.id,
      chat: c.chat,
      offDuty: isConciergeOffDuty(c.chat as any),
      state: getConciergeState(c.chat as any),
      active: isChatActiveDangerous(c.chat as any),
    }) + '\n'
  )
}
