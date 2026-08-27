import { Component } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Tooltip } from './tooltip';

/**
 * The tooltip primitive (v4 `components/ui/Tooltip.tsx` at `0bd84139`). The
 * behaviour worth pinning down is the part the browser used to own: when the
 * bubble appears, when it stays, and when it goes.
 *
 * The five `describe('Tooltip')` tests mirror v4's `__tests__/unit/components/
 * tooltip.test.tsx` 1:1 (dwell, leave-close, pin-until-Escape, scroll-follow,
 * non-pinnable click). The "emitted-constant pins" block pins the module
 * constants BEHAVIOURALLY against the values emitted from v4's real source at
 * the pin (`/tmp/p4d132-emit.json`): VIEWPORT_MARGIN 8 / ANCHOR_GAP 6 /
 * CLOSE_GRACE_MS 120 / default delay 200 / default placement 'top'.
 *
 * Regen recipe (the v4-client-oracle pattern): the committed recorder
 * `harness/oracle/cases/tooltip-strings.test.tsx` renders the REAL
 * MessageActionBar/ConfirmationBadge/Tooltip and greps the Tooltip constants
 * from v4's source — its own header carries the /tmp-mirror jest invocation
 * (run from the v4 checkout, or from a pinned worktree while the regen rule
 * is PIN REQUIRED). Nothing in this file is retyped from prose.
 */

@Component({
  imports: [Tooltip],
  template: `<qt-tooltip content="Copy message"><button aria-label="copy">copy</button></qt-tooltip>`,
})
class PlainHost {}

@Component({
  imports: [Tooltip],
  template: `<qt-tooltip content="The long story" pinnable
    ><button aria-label="badge">badge</button></qt-tooltip
  >`,
})
class PinnableHost {}

const bubble = (): HTMLElement | null => document.body.querySelector('.qt-tooltip');

async function render<T>(host: new () => T): Promise<ComponentFixture<T>> {
  TestBed.configureTestingModule({ imports: [host] });
  const fixture = TestBed.createComponent(host);
  fixture.detectChanges();
  return fixture;
}

/** Advance fake time, then let the render + afterRenderEffect passes run. */
async function tick(fixture: ComponentFixture<unknown>, ms: number): Promise<void> {
  await vi.advanceTimersByTimeAsync(ms);
  fixture.detectChanges();
  await vi.advanceTimersByTimeAsync(0);
  fixture.detectChanges();
}

function anchor(fixture: ComponentFixture<unknown>): HTMLElement {
  return (fixture.nativeElement as HTMLElement).querySelector('qt-tooltip')!;
}

async function hover(fixture: ComponentFixture<unknown>): Promise<void> {
  anchor(fixture).dispatchEvent(new Event('pointerenter'));
  await tick(fixture, 250);
}

describe('Tooltip (v4 tooltip.test.tsx @ 0bd84139)', () => {
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
  });

  it('stays hidden until the pointer has dwelt on the trigger', async () => {
    const fixture = await render(PlainHost);

    expect(bubble()).toBeNull();

    anchor(fixture).dispatchEvent(new Event('pointerenter'));
    await tick(fixture, 0);
    expect(bubble()).toBeNull();

    await tick(fixture, 250);
    expect(bubble()!.textContent).toContain('Copy message');
  });

  it('closes when the pointer leaves', async () => {
    const fixture = await render(PlainHost);

    await hover(fixture);
    expect(bubble()).not.toBeNull();

    anchor(fixture).dispatchEvent(new Event('pointerleave'));
    await tick(fixture, 200);

    expect(bubble()).toBeNull();
  });

  it('keeps a pinned bubble open after the pointer leaves, until Escape', async () => {
    const fixture = await render(PinnableHost);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('button')!;

    trigger.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await tick(fixture, 0);
    expect(bubble()).not.toBeNull();

    anchor(fixture).dispatchEvent(new Event('pointerleave'));
    await tick(fixture, 500);
    expect(bubble()).not.toBeNull();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await tick(fixture, 0);
    expect(bubble()).toBeNull();
  });

  it('follows its anchor when the page scrolls under it', async () => {
    // jsdom measures nothing, so both rects are stood in for: the anchor moves,
    // the bubble keeps a constant size (v4's own test, verbatim in spirit).
    let anchorTop = 500;
    const rect = vi
      .spyOn(HTMLElement.prototype, 'getBoundingClientRect')
      .mockImplementation(function (this: HTMLElement) {
        const isBubble = this.classList.contains('qt-tooltip');
        const top = isBubble ? 0 : anchorTop;
        const height = isBubble ? 40 : 20;
        return {
          top,
          bottom: top + height,
          left: 100,
          right: 128,
          width: 28,
          height,
          x: 100,
          y: top,
          toJSON: () => ({}),
        } as DOMRect;
      });

    try {
      const fixture = await render(PlainHost);
      await hover(fixture);

      const el = bubble()!;
      // 500 (anchor top) − 40 (bubble height) − 6 (ANCHOR_GAP, emitted)
      expect(el.style.top).toBe('454px');

      anchorTop = 300;
      window.dispatchEvent(new Event('scroll'));
      await tick(fixture, 50);

      expect(el.style.top).toBe('254px');
    } finally {
      rect.mockRestore();
    }
  });

  it('does not respond to clicks when it is not pinnable', async () => {
    const fixture = await render(PlainHost);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('button')!;

    trigger.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await tick(fixture, 0);
    expect(bubble()).toBeNull();
  });
});

describe('Tooltip — the emitted-constant pins (p4d132-emit.json @ 0bd84139)', () => {
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
  });

  it('opens after exactly the 200 ms default dwell (DEFAULT_DELAY 200)', async () => {
    const fixture = await render(PlainHost);

    anchor(fixture).dispatchEvent(new Event('pointerenter'));
    await tick(fixture, 199);
    expect(bubble()).toBeNull();
    await tick(fixture, 1);
    expect(bubble()).not.toBeNull();
  });

  it('opens immediately on keyboard focus, no dwell', async () => {
    const fixture = await render(PlainHost);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('button')!;

    // React's onFocus is delegated focusin — the wrapped control gaining focus
    // opens the bubble with no timer in between (Tooltip.tsx:196 `onFocus={openNow}`).
    trigger.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
    await tick(fixture, 0);
    expect(bubble()).not.toBeNull();

    // And blur closes it at once (`onBlur` → closeNow when unpinned).
    trigger.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    await tick(fixture, 0);
    expect(bubble()).toBeNull();
  });

  it('survives a pointer exit for the 120 ms close grace, no longer (CLOSE_GRACE_MS 120)', async () => {
    const fixture = await render(PlainHost);
    await hover(fixture);

    anchor(fixture).dispatchEvent(new Event('pointerleave'));
    await tick(fixture, 119);
    expect(bubble()).not.toBeNull();
    await tick(fixture, 1);
    expect(bubble()).toBeNull();
  });

  it('flips to the bottom when the top would breach the viewport margin, and clamps left (VIEWPORT_MARGIN 8, ANCHOR_GAP 6)', async () => {
    // Anchor hugging the top-left corner: preferred 'top' placement would put
    // the bubble at 10 − 40 − 6 = −36 < 8, so it flips to bottom
    // (top = a.bottom + 6) and the centred left (0+14−50=−36) clamps to 8.
    const rect = vi
      .spyOn(HTMLElement.prototype, 'getBoundingClientRect')
      .mockImplementation(function (this: HTMLElement) {
        const isBubble = this.classList.contains('qt-tooltip');
        const top = isBubble ? 0 : 10;
        const height = isBubble ? 40 : 20;
        const width = isBubble ? 100 : 28;
        return {
          top,
          bottom: top + height,
          left: 0,
          right: width,
          width,
          height,
          x: 0,
          y: top,
          toJSON: () => ({}),
        } as DOMRect;
      });

    try {
      const fixture = await render(PlainHost);
      await hover(fixture);

      const el = bubble()!;
      expect(el.getAttribute('data-placement')).toBe('bottom');
      // a.bottom (30) + ANCHOR_GAP (6)
      expect(el.style.top).toBe('36px');
      // clamped to VIEWPORT_MARGIN
      expect(el.style.left).toBe('8px');
    } finally {
      rect.mockRestore();
    }
  });

  it('portals the bubble onto document.body, aria-hidden, role tooltip', async () => {
    const fixture = await render(PlainHost);
    await hover(fixture);

    const el = bubble()!;
    expect(el.parentElement).toBe(document.body);
    expect(el.getAttribute('role')).toBe('tooltip');
    // The trigger carries the same words as its accessible name, so the bubble
    // itself is decoration as far as assistive tech is concerned.
    expect(el.getAttribute('aria-hidden')).toBe('true');
  });
});
