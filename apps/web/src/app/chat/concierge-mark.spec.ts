import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ConciergeProvenance, ConciergeReason, ConciergeState } from './concierge-state';
import { ConciergeMark, ConciergeTooltipBody, conciergeMarkClasses } from './concierge-mark';
import { describeConciergeState } from './concierge-state-presentation';

/**
 * The Concierge mark (v4 `components/chat/ConciergeMark.tsx` at `4d370a90f`),
 * transcribed from v4's `__tests__/unit/components/chat/concierge-mark.test.tsx`
 * at `ce2f1dabf`.
 *
 * The mark reads the derived three-state, never a stored column, so what
 * matters here is that Moderated draws nothing, that the other two each get
 * their own tone — Unmoderated ONE tone whoever set it — and that the words
 * come from the one presentation table, provenance picking Unmoderated's
 * sentence.
 *
 * Two recorded adaptations of v4's corpus, neither a behaviour difference:
 *  - v4 asserts `expect(container).toBeEmptyDOMElement()` for Moderated. An
 *    Angular component always has a host element, so the Moderated case
 *    asserts the host renders NO child at all instead.
 *  - v4's `ChatCard — the Concierge mark` block lives with the card
 *    (`screens/salon/chat-card.spec.ts`) — the card is a different component
 *    in v5 and binds the list DTO directly (v5 has no `ChatCardData`).
 */

@Component({
  imports: [ConciergeMark],
  template: `<qt-concierge-mark
    [conciergeState]="state()"
    [conciergeSetBy]="setBy()"
    [conciergeReason]="reason()"
    [dangerCategories]="categories()"
    [className]="extra()"
  />`,
})
class MarkHost {
  readonly state = signal<ConciergeState>('moderated');
  readonly setBy = signal<ConciergeProvenance | undefined>(undefined);
  readonly reason = signal<ConciergeReason | null | undefined>(undefined);
  readonly categories = signal<string[] | undefined>(undefined);
  readonly extra = signal('');
}

@Component({
  imports: [ConciergeTooltipBody],
  template: `<qt-concierge-tooltip-body [description]="description()" />`,
})
class BodyHost {
  readonly description = signal(describeConciergeState('unmoderated', { setBy: 'operator' }));
}

function renderMark(
  state: ConciergeState,
  opts: {
    setBy?: ConciergeProvenance;
    reason?: ConciergeReason | null;
    categories?: string[];
    className?: string;
  } = {},
): ComponentFixture<MarkHost> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [MarkHost] });
  const fixture = TestBed.createComponent(MarkHost);
  fixture.componentInstance.state.set(state);
  if (opts.setBy !== undefined) fixture.componentInstance.setBy.set(opts.setBy);
  if (opts.reason !== undefined) fixture.componentInstance.reason.set(opts.reason);
  if (opts.categories !== undefined) fixture.componentInstance.categories.set(opts.categories);
  if (opts.className !== undefined) fixture.componentInstance.extra.set(opts.className);
  fixture.detectChanges();
  return fixture;
}

const root = (fixture: ComponentFixture<unknown>): HTMLElement => fixture.nativeElement;
const mark = (fixture: ComponentFixture<unknown>, label: string): HTMLElement =>
  root(fixture).querySelector(`[aria-label="${label}"]`)!;
const bubble = (): HTMLElement | null => document.body.querySelector('.qt-tooltip');

/** Advance fake time, then let the render + afterRenderEffect passes run. */
async function tick(fixture: ComponentFixture<unknown>, ms: number): Promise<void> {
  await vi.advanceTimersByTimeAsync(ms);
  fixture.detectChanges();
  await vi.advanceTimersByTimeAsync(0);
  fixture.detectChanges();
}

async function hover(fixture: ComponentFixture<unknown>): Promise<void> {
  root(fixture).querySelector('qt-tooltip')!.dispatchEvent(new Event('pointerenter'));
  await tick(fixture, 250);
}

describe('ConciergeMark (v4 concierge-mark.test.tsx @ ce2f1dabf)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders nothing for Moderated — the default wears no mark', () => {
    const fixture = renderMark('moderated');
    expect(root(fixture).querySelector('qt-concierge-mark')!.children.length).toBe(0);
    expect(root(fixture).querySelector('.qt-concierge-mark')).toBeNull();
  });

  for (const [state, label, modifier] of [
    ['unmoderated', 'Concierge: Unmoderated', ''],
    ['locked', 'Concierge: Locked', 'qt-concierge-mark-muted'],
  ] as const) {
    it(`marks ${state} with an asterisk labelled "${label}"`, () => {
      const fixture = renderMark(state);

      const el = mark(fixture, label);
      expect(el.textContent).toContain('*');
      expect(el.classList.contains('qt-concierge-mark')).toBe(true);
      // Danger is the base rule; only Locked adds a modifier.
      expect(el.className).toBe(['qt-concierge-mark', modifier].filter(Boolean).join(' '));
    });
  }

  it('keeps one tone for Unmoderated whoever set it — provenance is never a colour', () => {
    let fixture = renderMark('unmoderated', { setBy: 'concierge', reason: 'classifier' });
    expect(mark(fixture, 'Concierge: Unmoderated').className).toBe('qt-concierge-mark');

    fixture = renderMark('unmoderated', { setBy: 'operator', reason: 'manual' });
    expect(mark(fixture, 'Concierge: Unmoderated').className).toBe('qt-concierge-mark');
  });

  it("appends the caller's classes without losing the tone", () => {
    const fixture = renderMark('locked', { className: 'text-sm flex-shrink-0' });

    const el = mark(fixture, 'Concierge: Locked');
    for (const cls of ['qt-concierge-mark', 'qt-concierge-mark-muted', 'text-sm', 'flex-shrink-0']) {
      expect(el.classList.contains(cls)).toBe(true);
    }
  });

  it('carries no native title — the drawn tooltip would double up on it', () => {
    const fixture = renderMark('unmoderated');
    expect(mark(fixture, 'Concierge: Unmoderated').hasAttribute('title')).toBe(false);
  });

  describe('the tooltip', () => {
    beforeEach(() => {
      vi.useFakeTimers({
        toFake: [
          'setTimeout',
          'clearTimeout',
          'setInterval',
          'clearInterval',
          'requestAnimationFrame',
          'cancelAnimationFrame',
        ],
      });
    });
    afterEach(() => {
      vi.useRealTimers();
      TestBed.resetTestingModule();
    });

    for (const state of ['unmoderated', 'locked'] as ConciergeState[]) {
      it(`speaks the presentation table's words for ${state}`, async () => {
        const { title, detail, hint } = describeConciergeState(state);
        const fixture = renderMark(state);

        await hover(fixture);

        const text = bubble()!.textContent ?? '';
        expect(text).toContain(title);
        expect(text).toContain(detail);
        expect(text).toContain(hint);
      });
    }

    it("speaks the operator's sentence when the operator set Unmoderated", async () => {
      const fixture = renderMark('unmoderated', { setBy: 'operator', reason: 'manual' });

      await hover(fixture);

      const text = bubble()!.textContent ?? '';
      expect(text).toMatch(/opened the uncensored door yourself/);
      expect(text).not.toMatch(/The Concierge moved this chat/);
    });

    it("speaks the Concierge's sentence when the Concierge set Unmoderated", async () => {
      const fixture = renderMark('unmoderated', { setBy: 'concierge', reason: 'classifier' });

      await hover(fixture);

      const text = bubble()!.textContent ?? '';
      expect(text).toMatch(
        /The Concierge moved this chat to the uncensored desk on reading the conversation/,
      );
      expect(text).not.toMatch(/opened the uncensored door yourself/);
    });

    it("lists the classifier's categories when the classifier moved the chat", async () => {
      const fixture = renderMark('unmoderated', {
        setBy: 'concierge',
        reason: 'classifier',
        categories: ['NSFW', 'Violence'],
      });

      await hover(fixture);

      const text = bubble()!.textContent ?? '';
      expect(text).toContain('Categories');
      expect(text).toContain('NSFW, Violence');
    });

    it('omits the categories line when the operator set the state', async () => {
      const fixture = renderMark('unmoderated', { setBy: 'operator', categories: ['NSFW'] });

      await hover(fixture);

      expect(bubble()!.textContent).not.toContain('Categories');
    });

    it('omits the categories line on Locked', async () => {
      const fixture = renderMark('locked', { categories: ['NSFW'] });

      await hover(fixture);

      expect(bubble()!.textContent).not.toContain('Categories');
    });

    // v5-only (P4.D229 M2): a list payload carries NO refusal count (§S.1), so
    // the list mark must never claim one — the Concierge-after-refusals bubble
    // reads the count-free sentence. Only the header pill passes the count.
    it('never names a refusal count on a list mark — the payload has none', async () => {
      const fixture = renderMark('unmoderated', { setBy: 'concierge', reason: 'refusals' });

      await hover(fixture);

      expect(bubble()!.textContent).toContain(
        'The Concierge moved this chat to the uncensored desk after the usual providers refused it.',
      );
    });
  });
});

describe('conciergeMarkClasses — the emitted string', () => {
  // v4 asserts `mark.className` verbatim; Angular's `[class]` binding
  // deduplicates its tokens, so the string is asserted at its source instead.
  // These are v4's own expectations, one per drawn state.
  it('gives Unmoderated the base rule alone — never the base class twice', () => {
    expect(conciergeMarkClasses('unmoderated')).toBe('qt-concierge-mark');
  });

  it('gives Locked the muted modifier', () => {
    expect(conciergeMarkClasses('locked')).toBe('qt-concierge-mark qt-concierge-mark-muted');
  });

  it("appends the caller's classes last", () => {
    expect(conciergeMarkClasses('locked', 'text-sm flex-shrink-0')).toBe(
      'qt-concierge-mark qt-concierge-mark-muted text-sm flex-shrink-0',
    );
    expect(conciergeMarkClasses('unmoderated', 'text-sm flex-shrink-0')).toBe(
      'qt-concierge-mark text-sm flex-shrink-0',
    );
  });
});

describe('ConciergeTooltipBody', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders title, detail and hint, and drops an absent categories line', () => {
    TestBed.configureTestingModule({ imports: [BodyHost] });
    const fixture = TestBed.createComponent(BodyHost);
    fixture.detectChanges();

    const text = root(fixture).textContent ?? '';
    expect(text).toContain('Unmoderated');
    expect(text).toContain('opened the uncensored door yourself');
    expect(text).toContain("Change it from the Salon sidebar's Chat section.");
    expect(text).not.toContain('Categories');
  });
});

/**
 * v4 `homepage-components.test.tsx:633-643` — "still opens the chat when the
 * mark itself is clicked". The mark sits inside the row's link; it must not
 * swallow the click. Transcribed at the round's unification review, which
 * found the case had been dropped: the behaviour held (the Tooltip's anchor
 * click returns early when not pinnable and never prevents default), but
 * nothing pinned it, so a `pinnable` mark or a `preventDefault()` in the
 * primitive would have stopped every list asterisk from navigating unseen.
 */
@Component({
  imports: [ConciergeMark],
  template: `<a href="/salon/chat-1" (click)="onClick($event)"
    ><qt-concierge-mark conciergeState="unmoderated" [dangerCategories]="['NSFW']"
  /></a>`,
})
class LinkedMarkHost {
  /** What the LINK saw: one entry per click that reached it, with whether
   *  anything below it had already cancelled the navigation. */
  readonly seen: { defaultPrevented: boolean }[] = [];
  onClick(event: MouseEvent): void {
    // Record BEFORE cancelling: the mark and the tooltip anchor have had their
    // turn by the time the event bubbles up here, so `defaultPrevented` at this
    // instant is theirs alone. The cancel keeps jsdom from navigating.
    this.seen.push({ defaultPrevented: event.defaultPrevented });
    event.preventDefault();
  }
}

describe('ConciergeMark — inside a link', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('still opens the chat when the mark itself is clicked', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [LinkedMarkHost] });
    const fixture = TestBed.createComponent(LinkedMarkHost);
    fixture.detectChanges();
    const mark = fixture.nativeElement.querySelector('.qt-concierge-mark') as HTMLElement;
    expect(mark).not.toBeNull();
    mark.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    // The click reached the link (nothing stopped its propagation) and arrived
    // un-cancelled (nothing below the link prevented the navigation).
    expect(fixture.componentInstance.seen).toEqual([{ defaultPrevented: false }]);
  });
});
