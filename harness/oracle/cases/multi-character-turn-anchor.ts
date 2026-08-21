/**
 * Oracle case (P4.D96): `applyMultiCharacterTurnAnchor` + the exported
 * `GROUP_SCENE_DISCIPLINE` block.
 *
 * Drives the REAL exports of v4's
 * lib/services/chat-message/context-builder.service.ts (`e22f7b36` — the
 * anti-chorus restructure: both anchor routes now edit the system message, the
 * prose route pushing its identity instruction FIRST and both pushing the
 * discipline block, joined with `\n\n` and appended after a leading `\n\n`).
 *
 * Each row emits the FULL post-call message array so the Rust side diffs it
 * byte-for-byte; one row emits the discipline constant on its own.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/multi-character-turn-anchor.ts \
 *     > /tmp/oracle-multi-character-turn-anchor.ndjson
 */

import {
  applyMultiCharacterTurnAnchor,
  GROUP_SCENE_DISCIPLINE,
} from '@/lib/services/chat-message/context-builder.service'

type WireMsg = {
  role: string
  content: string
  name?: string
  thoughtSignature?: string
}

type Row =
  | { kind: 'discipline'; id: string; text: string }
  | {
      kind: 'anchor'
      id: string
      messages: WireMsg[]
      characterName: string
      usePrefill: boolean
      out: WireMsg[]
    }

const rows: Row[] = []

rows.push({ kind: 'discipline', id: 'constant', text: GROUP_SCENE_DISCIPLINE })

const anchor = (id: string, messages: WireMsg[], characterName: string, usePrefill: boolean) => {
  const working = messages.map(m => ({ ...m }))
  applyMultiCharacterTurnAnchor(working as never, characterName, usePrefill)
  rows.push({
    kind: 'anchor',
    id,
    messages,
    characterName,
    usePrefill,
    // Re-shape so absent optional keys stay absent on both sides.
    out: working.map(m => {
      const o: WireMsg = { role: m.role, content: m.content }
      if (m.name !== undefined) o.name = m.name
      if (m.thoughtSignature !== undefined) o.thoughtSignature = m.thoughtSignature
      return o
    }),
  })
}

/** A non-trivial formatted-message array: system + a real exchange. */
const scene = (): WireMsg[] => [
  { role: 'system', content: 'You are Marie, an aeronaut of some renown.' },
  { role: 'user', content: 'The balloon is ready.' },
  { role: 'assistant', content: '[Gaston] *He checks the ropes.*', name: 'Gaston' },
  { role: 'user', content: 'Marie, are we going up?' },
]

const noSystem = (): WireMsg[] => [
  { role: 'user', content: 'The balloon is ready.' },
  { role: 'assistant', content: '*He checks the ropes.*', thoughtSignature: 'sig-1' },
]

// --- the four route × system-presence combinations -------------------------
anchor('prose-with-system', scene(), 'Marie', false)
anchor('prefill-with-system', scene(), 'Marie', true)
anchor('prose-no-system', noSystem(), 'Marie', false)
anchor('prefill-no-system', noSystem(), 'Marie', true)

// --- system-message placement / duplication --------------------------------
anchor(
  'prose-system-not-first',
  [
    { role: 'user', content: 'Before the frame.' },
    { role: 'system', content: 'You are Marie.' },
    { role: 'user', content: 'After.' },
  ],
  'Marie',
  false
)
anchor(
  'prefill-first-of-two-systems',
  [
    { role: 'system', content: 'FIRST system.' },
    { role: 'system', content: 'SECOND system.' },
    { role: 'user', content: 'Hello.' },
  ],
  'Marie',
  true
)
anchor('prose-empty-system-content', [{ role: 'system', content: '' }], 'Marie', false)
anchor('prefill-empty-message-array', [], 'Marie', true)
anchor('prose-empty-message-array', [], 'Marie', false)

// --- the interpolated name -------------------------------------------------
anchor('prose-name-with-apostrophe', scene(), "M'Baku d'Or", false)
anchor('prefill-name-with-brackets', scene(), '[Marie]', true)
anchor('prose-name-non-ascii', scene(), 'Zoë Ångström', false)
anchor('prose-empty-name', scene(), '', false)

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n')
