/**
 * The Green Room dialog state + the controller seam (v4
 * `creation-progress-provider.tsx`). The reducer + the concrete controller live
 * in `green-room.state.ts`; this module holds only the shapes the New-Chat state
 * service depends on, so the state unit compiles without the dialog.
 */

import type { OutfitPreviewSlots } from '../../core/core-contract';

/** One scrolling activity-log entry (v4 `CreationProgressLogEntry`). */
export interface GreenRoomLogEntry {
  message: string;
  level: 'info' | 'warn' | 'error';
  ts: number;
}

/** One per-character wardrobe consultation panel (v4 `CreationProgressWardrobePanel`). */
export interface GreenRoomWardrobePanel {
  characterId: string;
  characterName: string;
  /** `null` while the character is still "consulting the wardrobe". */
  slots: OutfitPreviewSlots | null;
}

/** The accumulated dialog state (v4 `CreationProgressState`). */
export interface GreenRoomState {
  open: boolean;
  status: string;
  logs: GreenRoomLogEntry[];
  wardrobe: GreenRoomWardrobePanel[];
  phase: 'running' | 'done' | 'error';
  errorMessage: string | null;
}

/** The pristine dialog state (v4 `INITIAL`). */
export const GREEN_ROOM_INITIAL: GreenRoomState = {
  open: false,
  status: '',
  logs: [],
  wardrobe: [],
  phase: 'running',
  errorMessage: null,
};

/**
 * The controller the submit spine drives (v4 `CreationProgressContextValue`):
 * open the dialog for a `progressId`, then close it on the POST resolving, or
 * surface a failure with a Close button.
 */
export interface GreenRoomController {
  /** Open the dialog and start filtering the global stream for this `progressId`. */
  begin(progressId: string): void;
  /** Creation succeeded (the dispatch resolved) — close the dialog. */
  complete(): void;
  /** Creation failed — surface the message + a Close button (dialog stays open). */
  fail(message: string): void;
}
