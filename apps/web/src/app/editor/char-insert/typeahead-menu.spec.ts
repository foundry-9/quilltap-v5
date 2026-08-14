import { afterEach, describe, expect, it } from 'vitest';

import {
  resolvePlacement,
  TypeaheadMenu,
  typeaheadOptionId,
  type TypeaheadRow,
} from './typeahead-menu';

/**
 * The menu surface: v4 `MenuPortal`'s geometry rules and ARIA wiring.
 *
 * The geometry is asserted through {@link resolvePlacement} rather than through
 * the DOM because jsdom reports every box as zero — an in-DOM assertion would
 * pass no matter what the rule said. The pure function IS the rule; the render
 * path feeds it real boxes.
 *
 * @module editor/char-insert/typeahead-menu.spec
 */
describe('resolvePlacement — v4 MenuPortal geometry', () => {
  const base = {
    anchorTop: 100,
    anchorBottom: 120,
    anchorLeft: 40,
    menuHeight: 200,
    menuWidth: 300,
    viewportHeight: 800,
    viewportWidth: 1000,
  };

  it('opens below when there is room', () => {
    expect(resolvePlacement(base)).toEqual({ placement: 'below', align: 'left' });
  });

  it('flips above when the caret is near the bottom — the Salon composer case', () => {
    // A composer at the bottom of the window: 60px below, 700px above.
    expect(
      resolvePlacement({ ...base, anchorTop: 700, anchorBottom: 740, viewportHeight: 800 }),
    ).toMatchObject({ placement: 'above' });
  });

  it('stays below when there is no room ANYWHERE — above would be worse', () => {
    // Cramped both ways, but the caret is nearer the top: below still wins.
    expect(
      resolvePlacement({ ...base, anchorTop: 10, anchorBottom: 30, viewportHeight: 100 }),
    ).toMatchObject({ placement: 'below' });
  });

  it('right-aligns when a left-aligned menu would run off the edge', () => {
    expect(resolvePlacement({ ...base, anchorLeft: 900 })).toMatchObject({ align: 'right' });
  });

  it('left-aligns when it fits exactly', () => {
    expect(
      resolvePlacement({ ...base, anchorLeft: 700, menuWidth: 300, viewportWidth: 1000 }),
    ).toMatchObject({ align: 'left' });
  });
});

describe('TypeaheadMenu — the surface v4 MenuPortal renders', () => {
  // A failed assertion short-circuits the per-test `destroy()`, and the menu
  // lives on `document.body` — so without this one red test cascades into every
  // test after it, which reads as a much bigger breakage than it is.
  afterEach(() => {
    document.querySelectorAll('.qt-typeahead-anchor').forEach((node) => node.remove());
  });

  const rows: TypeaheadRow[] = [
    { key: '😄', glyph: '😄', label: 'grinning face', detail: ':smile:' },
    { key: '🎉', glyph: '🎉', label: 'party popper', detail: ':tada:' },
  ];

  function mount() {
    const target = document.createElement('div');
    document.body.appendChild(target);
    const picks: number[] = [];
    const menu = new TypeaheadMenu({
      listboxId: 'qt-emoji-typeahead',
      emptyLabel: 'No emoji found',
      activeDescendantTarget: target,
      onSelect: (index) => picks.push(index),
      onHighlight: () => undefined,
    });
    return { menu, target, picks, cleanup: () => target.remove() };
  }

  it('renders one option per row, with the glyph, label and detail', () => {
    const { menu, cleanup } = mount();
    menu.render(rows, 0, { left: 10, top: 20, bottom: 40 });

    const options = document.querySelectorAll('.qt-typeahead-option');
    expect(options).toHaveLength(2);
    expect(options[0].querySelector('.qt-typeahead-option-glyph')?.textContent).toBe('😄');
    expect(options[0].querySelector('.qt-typeahead-option-label')?.textContent).toBe(
      'grinning face',
    );
    expect(options[0].querySelector('.qt-typeahead-option-detail')?.textContent).toBe(':smile:');
    expect(options[0].getAttribute('aria-label')).toBe('grinning face');

    menu.destroy();
    cleanup();
  });

  it('marks the highlighted row and points the editor at it', () => {
    const { menu, target, cleanup } = mount();
    menu.render(rows, 1, { left: 10, top: 20, bottom: 40 });

    const options = document.querySelectorAll('.qt-typeahead-option');
    expect(options[0].getAttribute('aria-selected')).toBe('false');
    expect(options[1].getAttribute('aria-selected')).toBe('true');
    expect(options[1].classList.contains('qt-typeahead-option-active')).toBe(true);

    // Focus never leaves the composer, so the composer announces the option.
    expect(target.getAttribute('aria-activedescendant')).toBe(
      typeaheadOptionId('qt-emoji-typeahead', 1),
    );
    expect(target.getAttribute('aria-controls')).toBe('qt-emoji-typeahead');
    expect(target.getAttribute('aria-expanded')).toBe('true');

    menu.destroy();
    cleanup();
  });

  it('reuses the option elements when only the highlight moved (dogfood #85)', () => {
    // The rows must be the SAME DOM nodes across a selection-only render. v4
    // gets that from React's keyed reconciliation; v5 has to do it by hand, and
    // the first cut did not — it rebuilt every row, which under a stationary
    // mouse pointer makes the browser fire `mouseenter` on the new node beneath
    // it and drag the selection back to the hovered row. The arrows then did
    // nothing whenever the pointer rested over the menu.
    //
    // jsdom has no hover engine, so the symptom cannot be reproduced here; the
    // mechanism behind it can, and this is it. The live gesture is walked by
    // `composer-char-insert-flow.spec.ts`.
    const { menu, target, cleanup } = mount();
    menu.render(rows, 0, { left: 10, top: 20, bottom: 40 });
    const first = document.querySelectorAll('.qt-typeahead-option');
    const before = [first[0], first[1]];

    menu.render(rows, 1, { left: 10, top: 20, bottom: 40 });
    const after = document.querySelectorAll('.qt-typeahead-option');

    expect(after[0]).toBe(before[0]);
    expect(after[1]).toBe(before[1]);
    // …and the highlight really did move on those same nodes.
    expect(after[0].getAttribute('aria-selected')).toBe('false');
    expect(after[1].getAttribute('aria-selected')).toBe('true');
    expect(after[1].className).toContain('qt-typeahead-option-active');
    expect(after[0].className).not.toContain('qt-typeahead-option-active');
    expect(target.getAttribute('aria-activedescendant')).toBe(
      typeaheadOptionId('qt-emoji-typeahead', 1),
    );

    menu.destroy();
    cleanup();
  });

  it('rebuilds when the row set itself changed', () => {
    const { menu, cleanup } = mount();
    menu.render(rows, 0, { left: 10, top: 20, bottom: 40 });
    const before = document.querySelectorAll('.qt-typeahead-option')[0];

    // A different query — different keys, so the reuse path must not fire and
    // the new rows must be the ones rendered.
    menu.render([{ key: '🐱', glyph: '🐱', label: 'cat face', detail: ':cat:' }], 0, {
      left: 10,
      top: 20,
      bottom: 40,
    });
    const after = document.querySelectorAll('.qt-typeahead-option');

    expect(after).toHaveLength(1);
    expect(after[0]).not.toBe(before);
    expect(after[0].getAttribute('aria-label')).toBe('cat face');

    menu.destroy();
    cleanup();
  });

  it('rebuilds when an empty result follows a populated one', () => {
    // The empty state is a `.qt-typeahead-empty` div, not an option — the reuse
    // path must never try to write `aria-selected` onto it.
    const { menu, cleanup } = mount();
    menu.render(rows, 0, { left: 10, top: 20, bottom: 40 });
    menu.render([], -1, { left: 10, top: 20, bottom: 40 });

    expect(document.querySelectorAll('.qt-typeahead-option')).toHaveLength(0);
    expect(document.querySelector('.qt-typeahead-empty')?.textContent).toBe('No emoji found');

    // …and a repopulated list renders fresh options again.
    menu.render(rows, 0, { left: 10, top: 20, bottom: 40 });
    expect(document.querySelectorAll('.qt-typeahead-option')).toHaveLength(2);

    menu.destroy();
    cleanup();
  });

  it('shows the profile`s empty label when nothing matched', () => {
    const { menu, cleanup } = mount();
    menu.render([], -1, { left: 10, top: 20, bottom: 40 });

    expect(document.querySelector('.qt-typeahead-empty')?.textContent).toBe('No emoji found');
    expect(document.querySelectorAll('.qt-typeahead-option')).toHaveLength(0);

    menu.destroy();
    cleanup();
  });

  it('anchors at the caret box it is given', () => {
    const { menu, cleanup } = mount();
    menu.render(rows, 0, { left: 123, top: 45, bottom: 63 });

    const anchor = document.querySelector<HTMLElement>('.qt-typeahead-anchor')!;
    expect(anchor.style.position).toBe('fixed');
    expect(anchor.style.left).toBe('123px');
    expect(anchor.style.top).toBe('45px');
    expect(anchor.style.height).toBe('18px');

    menu.destroy();
    cleanup();
  });

  it('commits the clicked row and keeps the composer`s selection (mousedown prevented)', () => {
    const { menu, picks, cleanup } = mount();
    menu.render(rows, 0, { left: 10, top: 20, bottom: 40 });

    const second = document.querySelectorAll('.qt-typeahead-option')[1];
    const event = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
    second.dispatchEvent(event);

    expect(picks).toEqual([1]);
    expect(event.defaultPrevented).toBe(true);

    menu.destroy();
    cleanup();
  });

  it('takes the whole surface down on close, ARIA included', () => {
    const { menu, target, cleanup } = mount();
    menu.render(rows, 0, { left: 10, top: 20, bottom: 40 });
    expect(menu.isOpen).toBe(true);

    menu.close();

    expect(menu.isOpen).toBe(false);
    expect(document.querySelector('.qt-typeahead-menu')).toBeNull();
    // A shut menu must not leave the composer claiming an expanded listbox that
    // no longer exists.
    expect(target.hasAttribute('aria-activedescendant')).toBe(false);
    expect(target.hasAttribute('aria-controls')).toBe(false);
    expect(target.hasAttribute('aria-expanded')).toBe(false);

    cleanup();
  });
});
