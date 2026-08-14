import { describe, expect, it } from 'vitest';

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
