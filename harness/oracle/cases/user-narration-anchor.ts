/**
 * Oracle case (P4.D243): the chained-turn scene note — v4's REAL
 * `lib/chat/context/user-narration-anchor.ts` (`ca363178d`).
 *
 * Each row carries the input as data (the `nameForParticipant` callback is a
 * corpus name map: `names: null` passes NO callback, `names: {}` passes one
 * that names nobody), the returned string, and every `logger.debug` call the
 * row made — captured by wrapping the REAL logger instance the module imports,
 * so the line's message, field VALUES and field ORDER are v4's own.
 *
 * Corpus: v4's eight unit cases (`__tests__/unit/lib/chat/context/
 * user-narration-anchor.test.ts`) verbatim, plus the JS-truthiness rows the
 * port has to get right by hand (empty-string ids / participant ids / seat
 * names, uppercase roles, a second human seat, a human id outside the window,
 * `hasNewUserMessage` with ids, null vs undefined id sets).
 *
 * Run from inside a v4 checkout AT the pin (the module does not exist at the
 * `97b25fc53` baseline):
 *   cd <v4 checkout at ca363178d>
 *   PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *     npx tsx ~/source/quilltap-v5/harness/oracle/cases/user-narration-anchor.ts \
 *     > /tmp/oracle-user-narration-anchor.ndjson
 */

import { logger } from '@/lib/logger'
import {
  buildUserNarrationAnchor,
  renderUserNarrationAnchor,
} from '@/lib/chat/context/user-narration-anchor'

type WinRow = { role: string; id?: string; participantId?: string | null }

type Input = {
  isMultiCharacter: boolean
  hasNewUserMessage: boolean
  historyWindow: WinRow[]
  /** `undefined` (key absent) / `null` / a list. */
  humanTurnMessageIds?: string[] | null
  userName: string
  /** `null` → no callback; an object → a callback over it. */
  names: Record<string, string> | null
}

const captured: Array<{ message: string; context: unknown }> = []
const realDebug = logger.debug.bind(logger)
;(logger as unknown as { debug: (m: string, c?: unknown) => void }).debug = (
  message: string,
  context?: unknown,
) => {
  captured.push({ message, context })
}

const out = (line: unknown) => process.stdout.write(JSON.stringify(line) + '\n')

out({ kind: 'render', id: 'render-owen', userName: 'Owen', text: renderUserNarrationAnchor('Owen') })
out({ kind: 'render', id: 'render-odd', userName: "D'Arcy [the] Ünbound", text: renderUserNarrationAnchor("D'Arcy [the] Ünbound") })
out({ kind: 'render', id: 'render-empty', userName: '', text: renderUserNarrationAnchor('') })

const row = (id: string, input: Input) => {
  captured.length = 0
  const names = input.names
  const text = buildUserNarrationAnchor({
    isMultiCharacter: input.isMultiCharacter,
    hasNewUserMessage: input.hasNewUserMessage,
    historyWindow: input.historyWindow,
    humanTurnMessageIds:
      input.humanTurnMessageIds === undefined
        ? undefined
        : input.humanTurnMessageIds === null
          ? null
          : new Set(input.humanTurnMessageIds),
    userName: input.userName,
    nameForParticipant: names === null ? undefined : (pid: string) => names[pid],
  })
  out({ kind: 'build', id, input, text, logs: captured.slice() })
}

const base = (): Input => ({
  isMultiCharacter: true,
  hasNewUserMessage: false,
  historyWindow: [
    { role: 'USER', id: 'u1', participantId: 'p-user' },
    { role: 'ASSISTANT', id: 'a1', participantId: 'p-a' },
    { role: 'USER', id: 'u2', participantId: 'p-user' },
    { role: 'USER', id: 'a2', participantId: 'p-b' },
  ],
  humanTurnMessageIds: ['u1', 'u2'],
  userName: 'Owen',
  names: null,
})

// --- v4's eight unit cases -------------------------------------------------
row('v4-applies', base())
row('v4-single-character', { ...base(), isMultiCharacter: false })
row('v4-first-responder', { ...base(), hasNewUserMessage: true })
row('v4-no-human-elsewhere', { ...base(), humanTurnMessageIds: ['elsewhere'] })
row('v4-no-human-undefined', { ...base(), humanTurnMessageIds: undefined })
row('v4-no-human-empty', { ...base(), humanTurnMessageIds: [] })
row('v4-staff-whisper-only', {
  ...base(),
  historyWindow: [
    { role: 'USER', id: 'u2', participantId: 'p-user' },
    { role: 'USER', id: 'host-1', participantId: null },
  ],
})
row('v4-role-assistant-no-pid', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u2' },
    { role: 'assistant', id: 'a9' },
  ],
})
row('v4-seat-wins', {
  ...base(),
  historyWindow: [
    { role: 'USER', id: 'u1', participantId: 'p-user-2' },
    { role: 'USER', id: 'u2', participantId: 'p-user' },
    { role: 'ASSISTANT', id: 'a1', participantId: 'p-a' },
  ],
  userName: 'Alex',
  names: { 'p-user': 'Owen', 'p-user-2': 'Alex' },
})
row('v4-unseated-fallback', { ...base(), userName: 'Alex', names: {} })

// --- the truthiness / trap rows ---------------------------------------------
row('null-id-set', { ...base(), humanTurnMessageIds: null })
row('empty-string-pid-after', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u2' },
    { role: 'user', id: 'x', participantId: '' },
  ],
})
row('empty-string-id-with-pid-after', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u2' },
    { role: 'user', id: '', participantId: 'p-b' },
  ],
})
row('empty-string-id-never-matches', {
  ...base(),
  humanTurnMessageIds: ['', 'u9'],
  historyWindow: [
    { role: 'user', id: '', participantId: 'p-user' },
    { role: 'assistant', id: 'a1', participantId: 'p-a' },
  ],
})
row('second-human-seat-after', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u1', participantId: 'p-user' },
    { role: 'user', id: 'u2', participantId: 'p-user-2' },
  ],
})
row('uppercase-assistant-no-id', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u2' },
    { role: 'ASSISTANT' },
  ],
})
row('mixed-case-assistant', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u1', participantId: 'p-user' },
    { role: 'AssIstant', id: 'a1' },
  ],
})
row('removed-seat-falls-back', { ...base(), userName: 'Alex', names: { 'p-other': 'Zed' } })
row('empty-seat-name-falls-back', { ...base(), userName: 'Alex', names: { 'p-user': '' } })
row('seat-name-equals-user-name', { ...base(), userName: 'Owen', names: { 'p-user': 'Owen' } })
row('seat-name-differs', { ...base(), userName: 'Alex', names: { 'p-user': 'Owen' } })
row('human-row-without-pid-callback-unused', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u2' },
    { role: 'user', id: 'c1', participantId: 'p-b' },
  ],
  userName: 'Alex',
  names: { 'p-b': 'Bea' },
})
row('last-human-not-last-user-role', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u1', participantId: 'p-user' },
    { role: 'user', id: 'c1', participantId: 'p-b' },
    { role: 'user', id: 'c2', participantId: 'p-c' },
  ],
  names: { 'p-user': 'Owen' },
})
row('human-id-outside-window', {
  ...base(),
  humanTurnMessageIds: ['u-trimmed'],
})
row('has-new-with-ids', { ...base(), hasNewUserMessage: true, names: { 'p-user': 'Owen' } })
row('human-last-nothing-after', {
  ...base(),
  historyWindow: base().historyWindow.slice(0, 3),
})
row('empty-window', { ...base(), historyWindow: [] })
row('empty-user-name-fallback', { ...base(), userName: '', names: {} })
row('only-human-rows', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u1' },
    { role: 'user', id: 'u2' },
  ],
})
row('whisper-then-character', {
  ...base(),
  historyWindow: [
    { role: 'user', id: 'u2', participantId: 'p-user' },
    { role: 'user', id: 'host-1', participantId: null },
    { role: 'assistant', id: 'a3', participantId: 'p-a' },
  ],
  names: { 'p-user': 'Owen' },
})

;(logger as unknown as { debug: unknown }).debug = realDebug
