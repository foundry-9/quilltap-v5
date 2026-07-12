/**
 * The Green Room frame reducer (v4 `creation-progress-provider.applyEvent` +
 * `appendLog` / `upsertWardrobe`). Folds one creation-progress frame off the
 * global event stream into the accumulated dialog state. Pure — unit-tested
 * against v4's transitions and copy.
 */

import type { CreationProgressFrame } from '../../core/core-contract';
import {
  type GreenRoomLogEntry,
  type GreenRoomState,
  type GreenRoomWardrobePanel,
} from './green-room.types';

/** Cap the scrolling log (v4 `MAX_LOGS`). */
const MAX_LOGS = 100;

function appendLog(logs: GreenRoomLogEntry[], entry: GreenRoomLogEntry): GreenRoomLogEntry[] {
  const next = [...logs, entry];
  return next.length > MAX_LOGS ? next.slice(next.length - MAX_LOGS) : next;
}

function upsertWardrobe(
  panels: GreenRoomWardrobePanel[],
  panel: GreenRoomWardrobePanel,
): GreenRoomWardrobePanel[] {
  const idx = panels.findIndex((p) => p.characterId === panel.characterId);
  if (idx === -1) return [...panels, panel];
  const next = [...panels];
  // Keep an already-resolved outfit if a later frame lacks slots.
  next[idx] = { ...next[idx], ...panel, slots: panel.slots ?? next[idx].slots };
  return next;
}

/**
 * Apply one creation-progress frame (v4 `applyEvent`). A no-op while the dialog
 * is closed; branches on `kind`. `ts` falls back to 0 (the field is optional on
 * the flattened envelope) so a malformed frame still folds in.
 */
export function applyGreenRoomFrame(
  prev: GreenRoomState,
  frame: CreationProgressFrame,
): GreenRoomState {
  if (!prev.open) return prev;
  const ts = frame.ts ?? 0;
  switch (frame.kind) {
    case 'status':
      return {
        ...prev,
        status: frame.message ?? prev.status,
        logs: appendLog(prev.logs, { message: frame.message ?? '', level: 'info', ts }),
      };
    case 'log':
      return {
        ...prev,
        logs: appendLog(prev.logs, {
          message: frame.message ?? '',
          level: frame.level ?? 'info',
          ts,
        }),
      };
    case 'wardrobe-start':
      return {
        ...prev,
        wardrobe: upsertWardrobe(prev.wardrobe, {
          characterId: frame.characterId ?? '',
          characterName: frame.characterName ?? '',
          slots: null,
        }),
        logs: appendLog(prev.logs, {
          message: `Consulting the wardrobe for ${frame.characterName ?? ''}…`,
          level: 'info',
          ts,
        }),
      };
    case 'wardrobe-result':
      return {
        ...prev,
        wardrobe: upsertWardrobe(prev.wardrobe, {
          characterId: frame.characterId ?? '',
          characterName: frame.characterName ?? '',
          slots: frame.slots ?? null,
        }),
        logs: appendLog(prev.logs, {
          message: `${frame.characterName ?? ''} is dressed and ready.`,
          level: 'info',
          ts,
        }),
      };
    case 'done':
      return { ...prev, phase: 'done', status: prev.status || 'The players are ready.' };
    case 'error':
      return {
        ...prev,
        phase: 'error',
        errorMessage: frame.message ?? null,
        status: 'Something went awry.',
      };
    default:
      return prev;
  }
}
