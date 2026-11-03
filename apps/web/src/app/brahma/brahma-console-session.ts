/**
 * One Brahma Console surface's conversation and model — a port of v4
 * `components/brahma-console/hooks/useBrahmaConsoleSession.ts` (`99463fc56`,
 * at `236c04e35`).
 *
 * The workspace tab and the always-on floating dialog each hold their OWN
 * session under their own localStorage key, so the two can sit on different
 * conversations at once (before `99463fc56` they shared one pointer and
 * fought over it). A plain factory — one instance per surface, never a root
 * singleton.
 *
 * @module brahma/brahma-console-session
 */

import { signal, type Signal } from '@angular/core';

import type { BrahmaConsoleApi } from './brahma-wire';

/** The floating dialog's remembered conversation (v4 `BrahmaConsoleDialog.tsx:23`). */
export const STORAGE_KEY_FLOATING_LAST_CHAT = 'quilltap:brahma-floating-last-id';
/** The workspace tab's remembered conversation (v4 `BrahmaConsoleView.tsx:17`). */
export const STORAGE_KEY_TAB_LAST_CHAT = 'quilltap:brahma-console-last-id';

export interface BrahmaConsoleSession {
  /** The open conversation, persisted under the surface's key. */
  readonly currentChatId: Signal<string | null>;
  setCurrentChatId(id: string | null): void;
  /** The connection profile (model) the open conversation talks to. */
  readonly activeConnectionProfileId: Signal<string | null>;
  /** Set the model locally (a chat load, the launcher) — does NOT PATCH. */
  setActiveConnectionProfileId(id: string | null): void;
  /**
   * Switch the model: set locally FIRST, then — with a conversation open —
   * PATCH it so the same chat continues on the new engine (v4 `:58-71`).
   */
  setModel(connectionProfileId: string): Promise<void>;
}

/** v4 `useBrahmaConsoleSession(storageKey)`. */
export function createBrahmaConsoleSession(
  storageKey: string,
  api: BrahmaConsoleApi,
): BrahmaConsoleSession {
  const currentChatId = signal<string | null>(readStoredChatId(storageKey));
  const activeConnectionProfileId = signal<string | null>(null);

  return {
    currentChatId: currentChatId.asReadonly(),
    setCurrentChatId(id) {
      currentChatId.set(id);
      try {
        if (id) localStorage.setItem(storageKey, id);
        else localStorage.removeItem(storageKey);
      } catch {
        /* ignore */
      }
    },
    activeConnectionProfileId: activeConnectionProfileId.asReadonly(),
    setActiveConnectionProfileId(id) {
      activeConnectionProfileId.set(id);
    },
    async setModel(connectionProfileId) {
      // Optimistically reflect the switch; the same chat continues.
      activeConnectionProfileId.set(connectionProfileId);
      const chatId = currentChatId();
      if (!chatId) return;
      try {
        await api.setModel(chatId, connectionProfileId);
      } catch (error) {
        console.error('Failed to switch Brahma Console model:', error);
      }
    },
  };
}

/** The persisted id, tolerating a storage-less environment (v4 `:41-47`). */
function readStoredChatId(storageKey: string): string | null {
  try {
    return localStorage.getItem(storageKey) || null;
  } catch {
    return null;
  }
}
