import { afterEach, describe, expect, it, vi } from 'vitest';

import { activateOnKey } from './activate-on-key';

/** P4.D358 — v4 `components/ui/activate-on-key.ts` (`dff76ebef`). */
function setup() {
  const outer = document.createElement('span');
  outer.setAttribute('role', 'button');
  outer.tabIndex = 0;
  const inner = document.createElement('i');
  outer.appendChild(inner);
  document.body.appendChild(outer);
  const clicked = vi.fn();
  outer.addEventListener('click', clicked);
  outer.addEventListener('keydown', activateOnKey);
  return { outer, inner, clicked };
}

function press(target: HTMLElement, key: string): KeyboardEvent {
  const ev = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
  target.dispatchEvent(ev);
  return ev;
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('activateOnKey (v4 activate-on-key.ts)', () => {
  it('clicks on Enter and prevents the default', () => {
    const { outer, clicked } = setup();
    const ev = press(outer, 'Enter');
    expect(clicked).toHaveBeenCalledTimes(1);
    expect(ev.defaultPrevented).toBe(true);
  });

  it('clicks on Space and prevents the default (no page scroll)', () => {
    const { outer, clicked } = setup();
    const ev = press(outer, ' ');
    expect(clicked).toHaveBeenCalledTimes(1);
    expect(ev.defaultPrevented).toBe(true);
  });

  it('ignores any other key', () => {
    const { outer, clicked } = setup();
    const ev = press(outer, 'a');
    expect(clicked).not.toHaveBeenCalled();
    expect(ev.defaultPrevented).toBe(false);
  });

  it('ignores a key bubbled up from a child (target !== currentTarget)', () => {
    const { inner, clicked } = setup();
    const ev = press(inner, 'Enter');
    expect(clicked).not.toHaveBeenCalled();
    expect(ev.defaultPrevented).toBe(false);
  });
});
