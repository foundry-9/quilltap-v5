/**
 * The Green Room controller (v4 `CreationProgressProvider`). Drives the blocking
 * chat-creation status dialog off the ONE global event stream — no bespoke SSE
 * route (D3/D6): on {@link begin} it subscribes to {@link CoreClient.events$},
 * filtering frames scope-tagged with the submit's `progressId`, and folds each
 * through {@link applyGreenRoomFrame}. The server buffers + replays a
 * `progressId`'s backlog to a late subscriber; our stream is already open, so
 * frames arrive live.
 *
 * The dispatch POST resolving — not the `done` frame — is the authoritative
 * ready signal ({@link complete}). Provided in root so the page's dialog and the
 * `NewChatState` share one instance.
 */

import { Injectable, inject, signal, type Signal } from '@angular/core';
import type { Subscription } from 'rxjs';

import { CoreClient } from '../../core/core-client';
import { applyGreenRoomFrame } from './green-room.reducer';
import { GREEN_ROOM_INITIAL, type GreenRoomController, type GreenRoomState } from './green-room.types';

@Injectable({ providedIn: 'root' })
export class GreenRoomStore implements GreenRoomController {
  private readonly core = inject(CoreClient);
  private readonly _state = signal<GreenRoomState>(GREEN_ROOM_INITIAL);
  private sub: Subscription | null = null;
  private progressId: string | null = null;

  /** The dialog reads this (v4's `CreationProgressState`). */
  readonly state: Signal<GreenRoomState> = this._state.asReadonly();

  begin(progressId: string): void {
    this.sub?.unsubscribe();
    this.progressId = progressId;
    this._state.set({
      ...GREEN_ROOM_INITIAL,
      open: true,
      status: 'Fetching the players from the green room…',
    });
    // The global EventSource is already connected; filter its frames by id.
    this.sub = this.core.events$.subscribe((frame) => {
      if (frame.progressId !== this.progressId || !frame.kind) return;
      this._state.update((prev) => applyGreenRoomFrame(prev, frame));
    });
  }

  complete(): void {
    this.close();
  }

  fail(message: string): void {
    // Stop streaming but KEEP the dialog open so the user can read the error.
    this.sub?.unsubscribe();
    this.sub = null;
    this._state.update((prev) =>
      prev.open ? { ...prev, phase: 'error', errorMessage: message, status: 'Something went awry.' } : prev,
    );
  }

  /** Dismiss the dialog and stop streaming (the error-state Close button). */
  close(): void {
    this.sub?.unsubscribe();
    this.sub = null;
    this.progressId = null;
    this._state.set(GREEN_ROOM_INITIAL);
  }
}
