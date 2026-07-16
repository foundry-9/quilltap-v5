import { inject, Injectable } from '@angular/core';

import { apiUrl } from '../core/api-url';
import { CoreClient } from '../core/core-client';
import type { PtySessionMeta } from './terminal-protocol';

/** The Salon's layout state (v4 `TerminalMode`): off / side-by-side / maximized. */
export type TerminalMode = 'normal' | 'split' | 'focus';

/** The subset of chat fields the terminal pane persists (v4 `persistChatTerminalState`). */
export interface TerminalPaneState {
  terminalMode: TerminalMode;
  activeTerminalSessionId: string | null;
  /** Vertical split when doc + terminal both stack. The horizontal
   *  chat|pane `dividerPosition` is owned by Document Mode (v4; the P4.6x
   *  ownership move) — the terminal no longer reads or persists it. */
  rightPaneVerticalSplit: number;
}

/** A session is "live" while its PTY hasn't exited (v4 `isLiveSession`). */
export function isLiveSession(meta: PtySessionMeta | null): boolean {
  if (!meta) return false;
  return !meta.exitedAt;
}

interface SessionListResponse {
  sessions: PtySessionMeta[];
}
interface SessionGetResponse {
  session: PtySessionMeta;
  ringBuffer: string | null;
}
interface SpawnResponse {
  session: PtySessionMeta;
}

/**
 * The terminal REST client (v4 `terminalModeApi.ts`) — a thin wrapper over the
 * frozen `quilltap-web` terminal routes (`/api/v1/terminals*`). The pane-state
 * persistence rides the SPA's dispatch envelope (`chatUpdate`) rather than v4's
 * bespoke chat PUT, since the terminal endpoints are the only REST surface the
 * SPA touches directly; everything else goes through {@link CoreClient}.
 */
@Injectable({ providedIn: 'root' })
export class TerminalApi {
  private readonly core = inject(CoreClient);

  async listTerminalSessions(chatId: string): Promise<PtySessionMeta[]> {
    const res = await fetch(apiUrl(`/api/v1/terminals?chatId=${encodeURIComponent(chatId)}`));
    const data = await parseJson<SessionListResponse>(res, 'Failed to list terminal sessions');
    return data.sessions ?? [];
  }

  async getTerminalSession(sessionId: string): Promise<PtySessionMeta | null> {
    const res = await fetch(apiUrl(`/api/v1/terminals/${sessionId}`));
    if (res.status === 404) return null;
    const data = await parseJson<SessionGetResponse>(res, 'Failed to load terminal session');
    return data.session;
  }

  async spawnTerminalSession(chatId: string): Promise<PtySessionMeta> {
    const res = await fetch(apiUrl('/api/v1/terminals'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ chatId }),
    });
    const data = await parseJson<SpawnResponse>(res, 'Failed to spawn terminal session');
    return data.session;
  }

  async killTerminalSession(sessionId: string): Promise<void> {
    const res = await fetch(apiUrl(`/api/v1/terminals/${sessionId}?action=kill`), {
      method: 'POST',
    });
    await expectOk(res, 'Failed to terminate session');
  }

  /** Persist the pane state onto the chat record (v4 `persistChatTerminalState`). */
  async persistChatTerminalState(
    chatId: string,
    updates: Partial<TerminalPaneState>,
  ): Promise<void> {
    const resp = await this.core.dispatch({ type: 'chatUpdate', chatId, chat: updates });
    if (resp.type === 'error') {
      throw new Error(resp.data.message);
    }
  }
}

async function expectOk(response: Response, fallbackMessage: string): Promise<void> {
  if (!response.ok) {
    let message = fallbackMessage;
    try {
      const body = await response.text();
      if (body) message = body;
    } catch {
      // swallow — the fallback stands
    }
    throw new Error(message);
  }
}

async function parseJson<T>(response: Response, fallbackMessage: string): Promise<T> {
  await expectOk(response, fallbackMessage);
  return response.json() as Promise<T>;
}
