import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ConciergeState } from './concierge-state';
import { ConciergeMark, ConciergeTooltipBody, conciergeMarkClasses } from './concierge-mark';
import { describeConciergeState } from './concierge-state-presentation';

/**
 * The Concierge mark (v4 `components/chat/ConciergeMark.tsx` at `c43d3b1b4`),
 * transcribed from v4's `__tests__/unit/components/chat/concierge-mark.test.tsx`.
 *
 * The mark reads the derived four-state, never the raw danger label, so what
 * matters here is that Monitored draws nothing, that the other three each get
 * their own tone, and that the words come from the one presentation table —
 * the same words the Salon header's pill and the sidebar's helper text use.
 *
 * Two recorded adaptations of v4's corpus, neither a behaviour difference:
 *  - v4 asserts `expect(container).toBeEmptyDOMElement()` for Monitored. An
 *    Angular component always has a host element, so the Monitored case
 *    asserts the host renders NO child at all instead.
 *  - v4's ChatCard block lives with the card (see `chat-card.spec.ts` and the
 *    other two list specs) — the card is a different component in v5 and the
 *    payload-carries-no-state arm is pinned there, where the `@if` lives.
 */

@Component({
  imports: [ConciergeMark],
  template: `<qt-concierge-mark
    [conciergeState]="state()"
    [dangerCategories]="categories()"
    [className]="extra()"
  />`,
})
class MarkHost {
  readonly state = signal<ConciergeState>('monitored');
  readonly categories = signal<string[] | undefined>(undefined);
  readonly extra = signal('');
}

@Component({
  imports: [ConciergeTooltipBody],
  template: `<qt-concierge-tooltip-body [description]="description()" />`,
})
class BodyHost {
  readonly description = signal(describeConciergeState('uncensored'));
}

function renderMark(
  state: ConciergeState,
  opts: { categories?: string[]; className?: string } = {},
): ComponentFixture<MarkHost> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [MarkHost] });
  const fixture = TestBed.createComponent(MarkHost);
  fixture.componentInstance.state.set(state);
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

describe('ConciergeMark (v4 concierge-mark.test.tsx @ c43d3b1b4)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders nothing for Monitored — the default wears no mark', () => {
    const fixture = renderMark('monitored');
    expect(root(fixture).querySelector('qt-concierge-mark')!.children.length).toBe(0);
    expect(root(fixture).querySelector('.qt-concierge-mark')).toBeNull();
  });

  for (const [state, label, modifier] of [
    ['flagged', 'Concierge: Flagged', ''],
    ['vouched', 'Concierge: Vouched Safe', 'qt-concierge-mark-muted'],
    ['uncensored', 'Concierge: Uncensored', 'qt-concierge-mark-info'],
  ] as const) {
    it(`marks ${state} with an asterisk labelled "${label}"`, () => {
      const fixture = renderMark(state);

      const el = mark(fixture, label);
      expect(el.textContent).toContain('*');
      expect(el.classList.contains('qt-concierge-mark')).toBe(true);
      // Danger is the base rule; only the two operator states add a modifier.
      expect(el.className).toBe(['qt-concierge-mark', modifier].filter(Boolean).join(' '));
    });
  }

  it("appends the caller's classes without losing the tone", () => {
    const fixture = renderMark('uncensored', { className: 'text-sm flex-shrink-0' });

    const el = mark(fixture, 'Concierge: Uncensored');
    for (const cls of ['qt-concierge-mark', 'qt-concierge-mark-info', 'text-sm', 'flex-shrink-0']) {
      expect(el.classList.contains(cls)).toBe(true);
    }
  });

  it('carries no native title — the drawn tooltip would double up on it', () => {
    const fixture = renderMark('flagged');
    expect(mark(fixture, 'Concierge: Flagged').hasAttribute('title')).toBe(false);
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

    for (const state of ['flagged', 'vouched', 'uncensored'] as ConciergeState[]) {
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

    it("lists the classifier's categories on a Flagged chat", async () => {
      const fixture = renderMark('flagged', { categories: ['NSFW', 'Violence'] });

      await hover(fixture);

      const text = bubble()!.textContent ?? '';
      expect(text).toContain('Categories');
      expect(text).toContain('NSFW, Violence');
    });

    it('omits the categories line on the operator states', async () => {
      const fixture = renderMark('vouched', { categories: ['NSFW'] });

      await hover(fixture);

      expect(bubble()!.textContent).not.toContain('Categories');
    });
  });
});

describe('conciergeMarkClasses — the emitted string', () => {
  // v4 asserts `mark.className` verbatim; Angular's `[class]` binding
  // deduplicates its tokens, so the string is asserted at its source instead.
  // These are v4's own three expectations, one per state.
  it('gives Flagged the base rule alone — never the base class twice', () => {
    expect(conciergeMarkClasses('flagged')).toBe('qt-concierge-mark');
  });

  it('gives Vouched Safe the muted modifier', () => {
    expect(conciergeMarkClasses('vouched')).toBe('qt-concierge-mark qt-concierge-mark-muted');
  });

  it('gives Uncensored the info modifier', () => {
    expect(conciergeMarkClasses('uncensored')).toBe('qt-concierge-mark qt-concierge-mark-info');
  });

  it("appends the caller's classes last", () => {
    expect(conciergeMarkClasses('uncensored', 'text-sm flex-shrink-0')).toBe(
      'qt-concierge-mark qt-concierge-mark-info text-sm flex-shrink-0',
    );
    expect(conciergeMarkClasses('flagged', 'text-sm flex-shrink-0')).toBe(
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
    expect(text).toContain('Uncensored');
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
    ><qt-concierge-mark conciergeState="flagged" [dangerCategories]="['NSFW']"
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
