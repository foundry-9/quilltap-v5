import { describe, expect, it } from 'vitest';

import type { CreationProgressFrame, OutfitPreviewSlots } from '../../core/core-contract';
import { applyGreenRoomFrame } from './green-room.reducer';
import { GREEN_ROOM_INITIAL, type GreenRoomState } from './green-room.types';

const OPEN: GreenRoomState = { ...GREEN_ROOM_INITIAL, open: true, status: 'Fetching the players from the green room…' };

function slots(): OutfitPreviewSlots {
  return {
    top: [{ id: 't1', title: 'Waistcoat', isComposite: false }],
    bottom: [],
    footwear: [],
    accessories: [],
  };
}

describe('applyGreenRoomFrame', () => {
  it('is a no-op while the dialog is closed', () => {
    const frame: CreationProgressFrame = { kind: 'status', message: 'x', ts: 1 };
    expect(applyGreenRoomFrame(GREEN_ROOM_INITIAL, frame)).toBe(GREEN_ROOM_INITIAL);
  });

  it('a status frame updates the headline and logs it', () => {
    const next = applyGreenRoomFrame(OPEN, { kind: 'status', message: 'Assembling the cast…', ts: 5 });
    expect(next.status).toBe('Assembling the cast…');
    expect(next.logs).toHaveLength(1);
    expect(next.logs[0]).toEqual({ message: 'Assembling the cast…', level: 'info', ts: 5 });
  });

  it('a log frame appends with its level (default info)', () => {
    const warn = applyGreenRoomFrame(OPEN, { kind: 'log', message: 'careful', level: 'warn', ts: 2 });
    expect(warn.logs[0].level).toBe('warn');
    const info = applyGreenRoomFrame(OPEN, { kind: 'log', message: 'note', ts: 3 });
    expect(info.logs[0].level).toBe('info');
  });

  it('wardrobe-start opens a spinning panel + logs the consultation copy', () => {
    const next = applyGreenRoomFrame(OPEN, {
      kind: 'wardrobe-start',
      characterId: 'c1',
      characterName: 'Aria',
      ts: 4,
    });
    expect(next.wardrobe).toEqual([{ characterId: 'c1', characterName: 'Aria', slots: null }]);
    expect(next.logs[0].message).toBe('Consulting the wardrobe for Aria…');
  });

  it('wardrobe-result resolves the same panel + logs the ready copy', () => {
    const started = applyGreenRoomFrame(OPEN, {
      kind: 'wardrobe-start',
      characterId: 'c1',
      characterName: 'Aria',
      ts: 4,
    });
    const resolved = applyGreenRoomFrame(started, {
      kind: 'wardrobe-result',
      characterId: 'c1',
      characterName: 'Aria',
      slots: slots(),
      ts: 6,
    });
    expect(resolved.wardrobe).toHaveLength(1);
    expect(resolved.wardrobe[0].slots).not.toBeNull();
    expect(resolved.logs.at(-1)?.message).toBe('Aria is dressed and ready.');
  });

  it('keeps a resolved outfit if a later frame lacks slots', () => {
    let s = applyGreenRoomFrame(OPEN, { kind: 'wardrobe-result', characterId: 'c1', characterName: 'Aria', slots: slots(), ts: 6 });
    s = applyGreenRoomFrame(s, { kind: 'wardrobe-start', characterId: 'c1', characterName: 'Aria', ts: 7 });
    expect(s.wardrobe[0].slots).not.toBeNull();
  });

  it('done sets the phase + the default ready copy', () => {
    const next = applyGreenRoomFrame({ ...OPEN, status: '' }, { kind: 'done', ts: 9 });
    expect(next.phase).toBe('done');
    expect(next.status).toBe('The players are ready.');
  });

  it('error sets the phase, the message, and the awry copy', () => {
    const next = applyGreenRoomFrame(OPEN, { kind: 'error', message: 'boom', ts: 9 });
    expect(next.phase).toBe('error');
    expect(next.errorMessage).toBe('boom');
    expect(next.status).toBe('Something went awry.');
  });

  it('caps the log at 100 entries (drops oldest)', () => {
    let s = OPEN;
    for (let i = 0; i < 105; i++) {
      s = applyGreenRoomFrame(s, { kind: 'log', message: `m${i}`, ts: i });
    }
    expect(s.logs).toHaveLength(100);
    expect(s.logs[0].message).toBe('m5');
    expect(s.logs.at(-1)?.message).toBe('m104');
  });
});
