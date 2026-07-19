/**
 * applyWorkspaceShortcut — the keyboard-shortcut applier (v4
 * `useWorkspaceShortcuts.ts`). Pure over a store surface; no DOM/TestBed.
 */

import { describe, expect, it, vi } from 'vitest';

import { createInitialState, workspaceReducer, type WorkspaceAction } from '../core/reducer';
import type { WorkspaceState } from '../workspace-contract';
import { applyWorkspaceShortcut, isEditableTarget, type ShortcutTarget } from './workspace-shortcuts';

function stub(state: WorkspaceState) {
  const calls: string[] = [];
  const target: ShortcutTarget = {
    state: () => state,
    setActive: (pane, id) => calls.push(`setActive:${pane}:${id}`),
    closeTab: (id) => calls.push(`closeTab:${id}`),
    unsplit: () => calls.push('unsplit'),
    splitTo: (id, pane) => calls.push(`splitTo:${id}:${pane}`),
  };
  return { target, calls };
}

function key(k: string, mods: Partial<KeyboardEvent> = {}): KeyboardEvent {
  const e = {
    key: k,
    altKey: true,
    ctrlKey: true,
    metaKey: false,
    shiftKey: false,
    target: null,
    preventDefault: vi.fn(),
    ...mods,
  } as unknown as KeyboardEvent;
  return e;
}

function build(...actions: WorkspaceAction[]): WorkspaceState {
  return actions.reduce(workspaceReducer, createInitialState('home'));
}

describe('applyWorkspaceShortcut', () => {
  const threeTabs = build(
    { type: 'OPEN_TAB', id: 'a', kind: 'aurora' },
    { type: 'OPEN_TAB', id: 'b', kind: 'prospero' },
  ); // order home,a,b ; active b

  it('ignores chords without Alt+(Ctrl|Cmd)', () => {
    const { target, calls } = stub(threeTabs);
    applyWorkspaceShortcut(key('ArrowRight', { altKey: false }), target);
    applyWorkspaceShortcut(key('ArrowRight', { ctrlKey: false, metaKey: false }), target);
    expect(calls).toEqual([]);
  });

  it('is inert while typing in a field', () => {
    const { target, calls } = stub(threeTabs);
    const input = { tagName: 'INPUT', isContentEditable: false } as unknown as HTMLElement;
    // jsdom instanceof: fake an HTMLElement via prototype
    Object.setPrototypeOf(input, HTMLElement.prototype);
    applyWorkspaceShortcut(key('ArrowRight', { target: input }), target);
    expect(calls).toEqual([]);
  });

  it('ArrowRight/Left cycle with wrap in the focused pane', () => {
    const { target, calls } = stub(threeTabs); // active b (index 2)
    applyWorkspaceShortcut(key('ArrowRight'), target); // wraps to index 0 (home)
    expect(calls).toEqual(['setActive:left:home']);
    calls.length = 0;
    applyWorkspaceShortcut(key('ArrowLeft'), target); // from b (2) → a (1)
    expect(calls).toEqual(['setActive:left:a']);
  });

  it('does not cycle with fewer than two tabs', () => {
    const { target, calls } = stub(createInitialState('home'));
    applyWorkspaceShortcut(key('ArrowRight'), target);
    expect(calls).toEqual([]);
  });

  it('digit keys jump to the nth tab', () => {
    const { target, calls } = stub(threeTabs);
    applyWorkspaceShortcut(key('1'), target);
    applyWorkspaceShortcut(key('3'), target);
    applyWorkspaceShortcut(key('9'), target); // out of range → ignored
    expect(calls).toEqual(['setActive:left:home', 'setActive:left:b']);
  });

  it('W closes the active tab', () => {
    const { target, calls } = stub(threeTabs);
    applyWorkspaceShortcut(key('w'), target);
    expect(calls).toEqual(['closeTab:b']);
  });

  it('backslash splits off the active tab when ≥2 remain, and unsplits when split', () => {
    const s1 = stub(threeTabs);
    applyWorkspaceShortcut(key('\\'), s1.target);
    expect(s1.calls).toEqual(['splitTo:b:right']);

    const splitState = build(
      { type: 'OPEN_TAB', id: 'a', kind: 'aurora' },
      { type: 'OPEN_TAB', id: 'b', kind: 'prospero', pane: 'right' },
    );
    const s2 = stub(splitState);
    applyWorkspaceShortcut(key('\\'), s2.target);
    expect(s2.calls).toEqual(['unsplit']);
  });

  it('backslash does not split when only one tab remains', () => {
    const { target, calls } = stub(createInitialState('home'));
    applyWorkspaceShortcut(key('\\'), target);
    expect(calls).toEqual([]);
  });
});

describe('isEditableTarget (jsdom)', () => {
  it('is true for input/textarea/select/contenteditable', () => {
    for (const tag of ['input', 'textarea', 'select']) {
      expect(isEditableTarget(document.createElement(tag))).toBe(true);
    }
    const ce = document.createElement('div');
    Object.defineProperty(ce, 'isContentEditable', { value: true, configurable: true });
    expect(isEditableTarget(ce)).toBe(true);
  });
  it('is false for null and non-editable elements', () => {
    expect(isEditableTarget(null)).toBe(false);
    // jsdom leaves `isContentEditable` undefined on a plain div (falsy); real
    // browsers return false. The v4 expression is preserved verbatim.
    expect(isEditableTarget(document.createElement('div'))).toBeFalsy();
  });
});
