import { afterEach, describe, expect, it } from 'vitest';

import { f6EscapeHatch, moveFocusOutOf } from './terminal';

/**
 * P4.D358 — the terminal's F6 escape hatch (v4 `components/terminal/
 * Terminal.tsx:99-108, :232-250`, `10e09b7c3`). jsdom lays nothing out, so
 * `offsetParent` is stubbed to stand in for "rendered".
 */
function visible(el: HTMLElement): HTMLElement {
  Object.defineProperty(el, 'offsetParent', { configurable: true, get: () => document.body });
  return el;
}

function setup() {
  document.body.innerHTML = `
    <button id="before">before</button>
    <div id="term"><textarea id="xterm-helper"></textarea></div>
    <button id="after">after</button>
    <button id="hidden">hidden</button>
  `;
  for (const id of ['before', 'xterm-helper', 'after']) visible(document.getElementById(id)!);
  const term = document.getElementById('term')!;
  (document.getElementById('xterm-helper') as HTMLElement).focus();
  return term;
}

function key(k: string, shiftKey = false, type = 'keydown'): KeyboardEvent {
  return new KeyboardEvent(type, { key: k, shiftKey, cancelable: true });
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('terminal F6 escape hatch (v4 Terminal.tsx)', () => {
  it('F6 moves focus to the next control after the terminal and swallows the key', () => {
    const handler = f6EscapeHatch(setup());
    const ev = key('F6');
    expect(handler(ev)).toBe(false);
    expect(ev.defaultPrevented).toBe(true);
    expect(document.activeElement?.id).toBe('after');
  });

  it('Shift+F6 moves focus to the previous control', () => {
    const handler = f6EscapeHatch(setup());
    handler(key('F6', true));
    expect(document.activeElement?.id).toBe('before');
  });

  it('every other key, and an F6 keyup, goes to the shell', () => {
    const handler = f6EscapeHatch(setup());
    expect(handler(key('Tab'))).toBe(true);
    expect(handler(key('F6', false, 'keyup'))).toBe(true);
    expect(document.activeElement?.id).toBe('xterm-helper');
  });

  it('skips unrendered controls (offsetParent null)', () => {
    const term = setup();
    document.getElementById('after')!.remove();
    moveFocusOutOf(term, 1);
    // #hidden has no offsetParent, so nothing follows: focus is dropped.
    expect(document.activeElement).toBe(document.body);
  });

  it('blurs when nothing follows', () => {
    const term = setup();
    document.getElementById('after')!.remove();
    document.getElementById('hidden')!.remove();
    moveFocusOutOf(term, 1);
    expect(document.activeElement).toBe(document.body);
  });
});
