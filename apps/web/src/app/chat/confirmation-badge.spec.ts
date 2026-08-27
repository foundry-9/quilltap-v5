import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { MessageDto } from '../core/core-contract';
import { ConfirmationBadge } from './confirmation-badge';

/**
 * The answer-confirmation badge (v4 `ConfirmationBadge.tsx` @ `0bd84139`).
 *
 * The six behaviour tests mirror v4's `tooltip.test.tsx` ConfirmationBadge
 * block 1:1; the emitted-string pins compare every state tuple —
 * (state, glyph, label, spoken) — and the pinned bubble's structure against
 * the table EMITTED from v4's REAL component at the pinned worktree
 * (`/tmp/p4d132-emit.json`; jest recorder `p4d132-emit.test.tsx`, run with
 * `QT_EMIT_OUT=/tmp/p4d132-emit.json npx jest p4d132-emit`, Node 24).
 * Nothing here is retyped from prose.
 */

function makeMessage(overrides: Partial<MessageDto> = {}): MessageDto {
  return {
    id: 'm1',
    role: 'ASSISTANT',
    content: 'Altitude is reported in feet.',
    tokenCount: null,
    promptTokens: null,
    completionTokens: null,
    createdAt: '2026-08-22T10:00:00.000Z',
    swipeGroupId: null,
    swipeIndex: null,
    participantId: 'p1',
    attachments: [],
    provider: null,
    modelName: null,
    targetParticipantIds: null,
    isSilentMessage: null,
    systemSender: null,
    systemKind: null,
    hostEvent: null,
    customAnnouncer: null,
    carinaMeta: null,
    pendingExternalPrompt: null,
    pendingExternalPromptFull: null,
    pendingExternalAttachments: null,
    reasoningContent: null,
    reasoningSegments: null,
    ...overrides,
  };
}

function render(msg: MessageDto): ComponentFixture<ConfirmationBadge> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ConfirmationBadge] });
  const fixture = TestBed.createComponent(ConfirmationBadge);
  fixture.componentRef.setInput('message', msg);
  fixture.detectChanges();
  return fixture;
}

const badge = (fixture: ComponentFixture<ConfirmationBadge>): HTMLButtonElement | null =>
  (fixture.nativeElement as HTMLElement).querySelector('button.qt-confirmation-badge');

const bubble = (): HTMLElement | null => document.querySelector('.qt-tooltip');

function click(fixture: ComponentFixture<ConfirmationBadge>): void {
  badge(fixture)!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  fixture.detectChanges();
}

describe('ConfirmationBadge (v4 tooltip.test.tsx @ 0bd84139)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows nothing when no check ever ran', () => {
    const fixture = render(makeMessage());
    expect((fixture.nativeElement as HTMLElement).children.length).toBe(0);
  });

  it('marks a reply the check left alone', () => {
    const fixture = render(makeMessage({ confirmed: true, confirmationChecked: true }));
    expect(badge(fixture)!.textContent).toContain('Vouched');
    expect(badge(fixture)!.getAttribute('data-confirmation-state')).toBe('vouched');
  });

  it('surfaces the notes and the original text of an amended reply', () => {
    const fixture = render(
      makeMessage({
        confirmed: true,
        confirmationChecked: true,
        confirmationRevised: true,
        confirmationNotes: 'The ledger excerpt shows a metric column.',
        confirmationOriginalContent: 'Altitude is reported in metres.',
      }),
    );

    const el = badge(fixture)!;
    expect(el.getAttribute('data-confirmation-state')).toBe('amended');
    // Everything the bubble says is also the badge's accessible name, so the
    // verdict survives for anyone who never hovers.
    expect(el.getAttribute('aria-label')).toContain('The ledger excerpt shows a metric column');
    expect(el.getAttribute('aria-label')).toContain('Altitude is reported in metres');

    click(fixture);
    expect(bubble()!.textContent).toContain('What looked off');
    expect(bubble()!.textContent).toContain('Originally written');
  });

  it('offers no pinning on a verdict with nothing further to say', () => {
    const fixture = render(makeMessage({ confirmed: true, confirmationChecked: true }));

    click(fixture);
    expect(bubble()).toBeNull();
  });

  it('recognises a reloaded unvetted reply from confirmationChecked alone', () => {
    const fixture = render(makeMessage({ confirmationChecked: true }));
    expect(badge(fixture)!.getAttribute('data-confirmation-state')).toBe('unvetted');
  });
});

describe('ConfirmationBadge — the emitted-string pins (p4d132-emit.json @ 0bd84139)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const NOTES = 'The ledger excerpt shows a metric column.';
  const ORIGINAL = 'Altitude is reported in metres.';
  const STOOD_BY_NOTES = 'The tower log disagrees on the runway heading.';

  /** [overrides, state, hasDetail, glyph, label, spoken] — emitted from v4. */
  const STATES: Array<[Partial<MessageDto>, string, string | null, string, string, string]> = [
    [
      { confirmed: true, confirmationChecked: true },
      'vouched',
      null,
      '✓',
      'Vouched',
      'Answer confirmation: Vouched. Checked against what the character recalled and looked up this turn — no contradictions found.',
    ],
    [
      {
        confirmed: true,
        confirmationChecked: true,
        confirmationRevised: true,
        confirmationNotes: NOTES,
        confirmationOriginalContent: ORIGINAL,
      },
      'amended',
      'true',
      '✎',
      'Amended',
      `Answer confirmation: Amended. On reflection the author corrected this reply to match the record. What looked off: ${NOTES} Originally written: ${ORIGINAL}`,
    ],
    [
      { confirmed: false, confirmationChecked: true, confirmationNotes: STOOD_BY_NOTES },
      'stood-by',
      'true',
      '!',
      'Stood by',
      `Answer confirmation: Stood by. The author was asked about apparent contradictions and stood by this reply unchanged. What looked off: ${STOOD_BY_NOTES}`,
    ],
    [
      { confirmationChecked: true },
      'unvetted',
      null,
      '—',
      'Unvetted',
      'Answer confirmation: Unvetted. This reply could not be checked — the verifier was unavailable or the check timed out.',
    ],
  ];

  it('pins every state tuple: state, has-detail, glyph, label, and the spoken join', () => {
    for (const [overrides, state, hasDetail, glyph, label, spoken] of STATES) {
      const fixture = render(makeMessage(overrides));
      const el = badge(fixture)!;
      expect(el.getAttribute('data-confirmation-state')).toBe(state);
      expect(el.getAttribute('data-has-detail')).toBe(hasDetail);
      expect(el.querySelector('.qt-confirmation-badge-glyph')!.textContent).toBe(glyph);
      expect(el.querySelector('.qt-confirmation-badge-label')!.textContent).toBe(label);
      expect(el.getAttribute('aria-label')).toBe(spoken);
      fixture.destroy();
      TestBed.resetTestingModule();
    }
  });

  it('pins the pinned bubble structure of an amended reply (emitted)', () => {
    const fixture = render(makeMessage(STATES[1][0]));
    click(fixture);
    const el = bubble()!;
    expect(el.querySelector('.qt-tooltip-title')!.textContent).toBe('Amended');
    expect(
      Array.from(el.querySelectorAll('.qt-tooltip-section-label')).map((n) => n.textContent),
    ).toEqual(['What looked off', 'Originally written']);
    expect(Array.from(el.querySelectorAll('.qt-tooltip-quote')).map((n) => n.textContent)).toEqual([
      NOTES,
      ORIGINAL,
    ]);
    expect(el.querySelector('.qt-tooltip-hint')!.textContent).toBe(
      'Click the badge to pin this note; Esc dismisses it.',
    );
    // The bubble carries data-pinned and answers Escape.
    expect(el.getAttribute('data-pinned')).toBe('true');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();
    expect(bubble()).toBeNull();
  });

  it('pins the stood-by bubble: one section, notes only (emitted)', () => {
    const fixture = render(makeMessage(STATES[2][0]));
    click(fixture);
    const el = bubble()!;
    expect(el.querySelector('.qt-tooltip-title')!.textContent).toBe('Stood by');
    expect(
      Array.from(el.querySelectorAll('.qt-tooltip-section-label')).map((n) => n.textContent),
    ).toEqual(['What looked off']);
    expect(Array.from(el.querySelectorAll('.qt-tooltip-quote')).map((n) => n.textContent)).toEqual([
      STOOD_BY_NOTES,
    ]);
  });
});
