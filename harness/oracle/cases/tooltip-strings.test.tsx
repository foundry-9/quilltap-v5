/**
 * P4.D132 parity-table EMISSION (not a test of v4 — a recorder).
 *
 * Renders v4's REAL MessageActionBar / ConfirmationBadge / Tooltip (v4
 * `0bd841394`) and writes the (tooltip content, aria-label) pairs, the badge
 * state tuples + spoken joins + bubble structure, and the Tooltip constants
 * (grepped from the real source file) to QT_EMIT_OUT as JSON. The v5 Angular
 * parity specs paste these bytes (`apps/web/src/app/ui/tooltip.spec.ts`,
 * `apps/web/src/app/chat/message-row.spec.ts`,
 * `apps/web/src/app/chat/confirmation-badge.spec.ts`); nothing there is
 * retyped by hand.
 *
 * Regenerate (from the v4 checkout — or, while the regen rule is PIN
 * REQUIRED, from a pinned worktree per drift-ledger §5.1 — Node 24; jest
 * ignores `/.claude/` paths, so the file is mirrored to /tmp first):
 *
 *   V5=~/source/quilltap-v5
 *   mkdir -p /tmp/qt-oracle-tooltip-strings
 *   cp $V5/harness/oracle/cases/tooltip-strings.test.tsx /tmp/qt-oracle-tooltip-strings/
 *   # .tsx: the automatic JSX transform resolves react/jsx-runtime FROM the
 *   # mirror dir, so it needs its own node_modules link (the .ts recorders
 *   # never did):
 *   ln -sfn ~/source/quilltap-server/node_modules /tmp/qt-oracle-tooltip-strings/node_modules
 *   cd ~/source/quilltap-server   # or the pinned worktree
 *   PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   QT_EMIT_OUT=/tmp/p4d132-emit.json \
 *     npx jest --silent --roots="$PWD" --roots=/tmp/qt-oracle-tooltip-strings \
 *       --testPathPatterns="tooltip-strings"
 *
 * @module harness/oracle/cases/tooltip-strings
 */
import { fireEvent, render, act } from '@testing-library/react'
import fs from 'fs'
import path from 'path'
import { MessageActionBar } from '@/app/salon/[id]/components/message-row/MessageActionBar'
import { ConfirmationBadge } from '@/app/salon/[id]/components/message-row/ConfirmationBadge'
import type { Message } from '@/app/salon/[id]/types'
import type { ParticipantData } from '@/components/chat/ParticipantCard'

const OUT = process.env.QT_EMIT_OUT || '/tmp/p4d132-emit.json'

function makeMessage(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm1',
    chatId: 'c1',
    role: 'ASSISTANT',
    content: 'Altitude is reported in feet.',
    createdAt: new Date('2026-08-22T10:00:00Z').toISOString(),
    ...overrides,
  } as Message
}

const participants: ParticipantData[] = [
  { id: 'p-other', type: 'CHARACTER', displayOrder: 0, isActive: true } as ParticipantData,
]

const noop = () => {}

/** Walk the icon row's tooltip anchors in DOM order; open each bubble by dwell
 *  and record (bubble text, trigger aria-label). */
function harvest(container: HTMLElement): Array<{ content: string; ariaLabel: string | null }> {
  const rows: Array<{ content: string; ariaLabel: string | null }> = []
  const anchors = container.querySelectorAll('.qt-chat-message-action-bar-icons .qt-tooltip-anchor')
  anchors.forEach(anchor => {
    const trigger = anchor.querySelector('button')
    if (!trigger) return
    fireEvent.pointerEnter(anchor)
    act(() => { jest.advanceTimersByTime(250) })
    const bubble = document.body.querySelector('.qt-tooltip')
    rows.push({
      content: bubble ? bubble.textContent ?? '' : '(no bubble)',
      ariaLabel: trigger.getAttribute('aria-label'),
    })
    fireEvent.pointerLeave(anchor)
    act(() => { jest.advanceTimersByTime(250) })
  })
  return rows
}

describe('P4.D132 emission', () => {
  beforeEach(() => { jest.useFakeTimers() })
  afterEach(() => { jest.runOnlyPendingTimers(); jest.useRealTimers() })

  it('emits the parity tables', () => {
    const emitted: Record<string, unknown> = {
      sha: '0bd841394 (pinned worktree at b121ac77f)',
    }

    // --- Tooltip constants, read from the real source file ---
    const src = fs.readFileSync(path.join(process.cwd(), 'components/ui/Tooltip.tsx'), 'utf8')
    const num = (re: RegExp) => Number((src.match(re) ?? [])[1])
    emitted.constants = {
      VIEWPORT_MARGIN: num(/const VIEWPORT_MARGIN = (\d+)/),
      ANCHOR_GAP: num(/const ANCHOR_GAP = (\d+)/),
      CLOSE_GRACE_MS: num(/const CLOSE_GRACE_MS = (\d+)/),
      DEFAULT_DELAY: num(/delay = (\d+)/),
      DEFAULT_PLACEMENT: (src.match(/placement = '(\w+)'/) ?? [])[1],
    }

    // --- MessageActionBar: render A (assistant, everything on) ---
    const shared = {
      viewSourceMessageIds: new Set<string>(),
      swipeState: { current: 1, total: 3 },
      showResendButton: false,
      hasLLMLogs: true,
      participantData: participants,
      onToggleSystemMessageExpanded: noop,
      onCopyContent: noop,
      onSaveImage: noop,
      onToggleSourceView: noop,
      onEditStart: noop,
      onDelete: noop,
      onGenerateSwipe: noop,
      onReattribute: noop,
      onViewLLMLogs: noop,
      onResend: noop,
      onSwitchSwipe: noop,
    }
    const assistantMsg = makeMessage({
      systemSender: 'pascal',
      swipeGroupId: 'sg1',
      participantId: 'p-self',
      attachments: [
        { id: 'a1', mimeType: 'image/png' },
        { id: 'a2', mimeType: 'image/png' },
      ],
    } as Partial<Message>)
    const a = render(<MessageActionBar {...shared} message={assistantMsg} />)
    emitted.assistantRow = harvest(a.container)
    a.unmount()

    // --- render B: user role, resend on, ONE image (singular save) ---
    const userMsg = makeMessage({
      role: 'USER',
      participantId: 'p-self',
      attachments: [{ id: 'a1', mimeType: 'image/png' }],
    } as Partial<Message>)
    const b = render(
      <MessageActionBar
        {...shared}
        swipeState={null}
        hasLLMLogs={false}
        showResendButton={true}
        message={userMsg}
      />
    )
    emitted.userRow = harvest(b.container)
    b.unmount()

    // --- render C: source view active (View rendered variant) ---
    const c = render(
      <MessageActionBar
        {...shared}
        swipeState={null}
        hasLLMLogs={false}
        viewSourceMessageIds={new Set(['m1'])}
        message={makeMessage({ participantId: 'p-self' })}
      />
    )
    emitted.sourceActiveRow = harvest(c.container)
    c.unmount()

    // --- ConfirmationBadge: the four states ---
    const badgeStates: Array<Record<string, unknown>> = []
    const cases: Array<[string, Partial<Message>]> = [
      ['vouched', { confirmed: true, confirmationChecked: true }],
      ['amended', {
        confirmed: true,
        confirmationChecked: true,
        confirmationRevised: true,
        confirmationNotes: 'The ledger excerpt shows a metric column.',
        confirmationOriginalContent: 'Altitude is reported in metres.',
      }],
      ['stood-by', {
        confirmed: false,
        confirmationChecked: true,
        confirmationNotes: 'The tower log disagrees on the runway heading.',
      }],
      ['unvetted', { confirmationChecked: true }],
    ]
    for (const [name, overrides] of cases) {
      const r = render(<ConfirmationBadge message={makeMessage(overrides)} />)
      const btn = r.container.querySelector('button.qt-confirmation-badge')!
      const glyph = btn.querySelector('.qt-confirmation-badge-glyph')!
      const label = btn.querySelector('.qt-confirmation-badge-label')!
      const row: Record<string, unknown> = {
        name,
        state: btn.getAttribute('data-confirmation-state'),
        hasDetail: btn.getAttribute('data-has-detail'),
        glyph: glyph.textContent,
        label: label.textContent,
        spoken: btn.getAttribute('aria-label'),
      }
      // Pin the bubble on the detailed states and record its structure.
      fireEvent.click(btn)
      const bubble = document.body.querySelector('.qt-tooltip')
      if (bubble) {
        row.bubble = {
          title: bubble.querySelector('.qt-tooltip-title')?.textContent,
          paragraphs: Array.from(bubble.querySelectorAll('.qt-tooltip-body > p'))
            .map(p => p.textContent),
          sectionLabels: Array.from(bubble.querySelectorAll('.qt-tooltip-section-label'))
            .map(p => p.textContent),
          quotes: Array.from(bubble.querySelectorAll('.qt-tooltip-quote'))
            .map(p => p.textContent),
          hint: bubble.querySelector('.qt-tooltip-hint')?.textContent ?? null,
        }
        fireEvent.keyDown(document, { key: 'Escape' })
      } else {
        row.bubble = null
      }
      badgeStates.push(row)
      r.unmount()
    }
    emitted.badge = badgeStates

    // The badge suppressed when no check ever ran (v4's own first test).
    const none = render(<ConfirmationBadge message={makeMessage()} />)
    emitted.badgeWhenUnchecked = none.container.innerHTML
    none.unmount()

    fs.writeFileSync(OUT, JSON.stringify(emitted, null, 2))
    expect(fs.existsSync(OUT)).toBe(true)
  })
})
