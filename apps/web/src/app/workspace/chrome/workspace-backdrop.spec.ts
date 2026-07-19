/**
 * arbitrateBackdrop — the v4 backdrop arbitration rule (v4 `workspace-backdrop.tsx`).
 */

import { describe, expect, it } from 'vitest';

import { createInitialState, workspaceReducer, type WorkspaceAction } from '../core/reducer';
import type { WorkspaceBackdropEntry, WorkspaceState } from '../workspace-contract';
import { arbitrateBackdrop } from './workspace-backdrop';

function build(...actions: WorkspaceAction[]): WorkspaceState {
  return actions.reduce(workspaceReducer, createInitialState('home'));
}

const salon = (url: string): WorkspaceBackdropEntry => ({ url, isSalon: true });
const page = (url: string): WorkspaceBackdropEntry => ({ url, isSalon: false });

describe('arbitrateBackdrop', () => {
  it('returns null when nothing is reported', () => {
    expect(arbitrateBackdrop(createInitialState('home'), {})).toBeNull();
  });

  it('unsplit: the active tab background fills the screen', () => {
    const s = build({ type: 'OPEN_TAB', id: 'a', kind: 'aurora' }); // active a
    const out = arbitrateBackdrop(s, { a: page('/p.png') });
    expect(out).toEqual({ fullUrl: '/p.png', leftUrl: null, rightUrl: null, splitPct: '50%' });
  });

  it('a Salon with a background wins full-screen even when split', () => {
    const s = build(
      { type: 'OPEN_TAB', id: 's', kind: 'salon', payload: { chatId: 'c1' } },
      { type: 'OPEN_TAB', id: 'a', kind: 'aurora', pane: 'right' },
    ); // left active s, right active a, focus right
    const out = arbitrateBackdrop(s, { s: salon('/story.png'), a: page('/side.png') });
    expect(out).toEqual({ fullUrl: '/story.png', leftUrl: null, rightUrl: null, splitPct: '50%' });
  });

  it('prefers the focused pane when both panes are Salons-with-backgrounds', () => {
    const s = build(
      { type: 'OPEN_TAB', id: 's1', kind: 'salon', payload: { chatId: 'c1' } },
      { type: 'OPEN_TAB', id: 's2', kind: 'salon', payload: { chatId: 'c2' }, pane: 'right' },
    ); // focus right (s2)
    expect(arbitrateBackdrop(s, { s1: salon('/left.png'), s2: salon('/right.png') })?.fullUrl).toBe(
      '/right.png',
    );
  });

  it('split with non-salon backgrounds: per-side layers + rounded split pct', () => {
    let s = build(
      { type: 'OPEN_TAB', id: 'a', kind: 'aurora' },
      { type: 'OPEN_TAB', id: 'b', kind: 'prospero', pane: 'right' },
    );
    s = workspaceReducer(s, { type: 'SET_SPLIT_RATIO', ratio: 0.62 });
    const out = arbitrateBackdrop(s, { a: page('/l.png'), b: page('/r.png') });
    expect(out).toEqual({ fullUrl: null, leftUrl: '/l.png', rightUrl: '/r.png', splitPct: '62%' });
  });
});
