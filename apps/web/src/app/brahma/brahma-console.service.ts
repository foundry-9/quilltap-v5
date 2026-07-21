/**
 * The Brahma Console state service (port of v4
 * `components/providers/brahma-console-provider.tsx`).
 *
 * The Brahma Console is a character-less, memory-free generic-LLM surface, so —
 * unlike a Help Chat — there is NO character/eligibility selection and NO
 * pathname tracking (the console is not page-aware). What it adds is the active
 * connection profile (model), switchable at any time, continuing the same chat.
 *
 * A `providedIn: 'root'` singleton so the rail entry (which flips `isOpen` /
 * holds the floating dialog) and the console body share one state — exactly v4's
 * app-level provider.
 *
 * @module brahma/brahma-console.service
 */

import { Injectable, computed, inject, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import { BrahmaConsoleApi } from './brahma-wire';

/** A connection profile as the model picker needs it (v4 `BrahmaConnectionProfile`). */
export interface BrahmaConnectionProfile {
  id: string;
  name: string;
  provider: string;
  modelName: string;
}

/** localStorage key for the last-open console chat (v4 `STORAGE_KEY_LAST_CHAT`). */
export const STORAGE_KEY_LAST_CHAT = 'quilltap:brahma-console-last-id';

@Injectable({ providedIn: 'root' })
export class BrahmaConsoleService {
  private readonly core = inject(CoreClient);
  private readonly api = inject(BrahmaConsoleApi);

  /** Whether the floating console dialog is open (v4 `isOpen`). */
  readonly isOpen = signal(false);

  /** The active console chat id — persisted to localStorage (v4 `currentChatId`). */
  readonly currentChatId = signal<string | null>(readStoredChatId());

  /** The connection profile (model) the open chat is talking to (v4). */
  readonly activeConnectionProfileId = signal<string | null>(null);

  /**
   * The user's connection profiles, shared with the settings surface via the
   * `['connectionProfiles']` query key (dedups). Mapped to the picker's shape.
   */
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connectionProfiles'],
    queryFn: async (): Promise<BrahmaConnectionProfile[]> => {
      const resp = await this.core.dispatchExpect(
        { type: 'connectionProfileList' },
        'connectionProfiles',
      );
      return resp.data.profiles.map((p) => ({
        id: p.id,
        name: p.name || '',
        provider: p.provider || '',
        modelName: p.modelName || '',
      }));
    },
  }));

  /** All of the user's connection profiles (for the model picker). */
  readonly profiles = computed<BrahmaConnectionProfile[]>(() => this.profilesQuery.data() ?? []);

  /** Whether the profile list is still loading. */
  readonly profilesLoading = computed(() => this.profilesQuery.isLoading());

  /** Whether the console can be opened at all — ≥1 connection profile (v4 L137). */
  readonly isEligible = computed(() => this.profiles().length > 0);

  /** Open the console to the launcher view so past chats are visible (v4). */
  openConsole(): void {
    // Always open to the launcher view (currentChatId = null).
    this.setCurrentChatId(null);
    this.isOpen.set(true);
  }

  /** Close the floating console. */
  closeConsole(): void {
    this.isOpen.set(false);
  }

  /** Set the active chat id and persist it (v4 `setCurrentChatId`). */
  setCurrentChatId(id: string | null): void {
    this.currentChatId.set(id);
    try {
      if (id) localStorage.setItem(STORAGE_KEY_LAST_CHAT, id);
      else localStorage.removeItem(STORAGE_KEY_LAST_CHAT);
    } catch {
      /* ignore */
    }
  }

  /** Set the active model locally (e.g. when a chat loads) — does NOT PATCH. */
  setActiveConnectionProfileId(id: string | null): void {
    this.activeConnectionProfileId.set(id);
  }

  /**
   * Switch the model for the current chat. Optimistically reflects the switch,
   * then PATCHes so the same chat continues with the new engine (v4 `setModel`).
   */
  async setModel(connectionProfileId: string): Promise<void> {
    // Optimistically reflect the switch; the same chat continues.
    this.activeConnectionProfileId.set(connectionProfileId);
    const chatId = this.currentChatId();
    if (!chatId) return;
    try {
      await this.api.setModel(chatId, connectionProfileId);
    } catch (error) {
      console.error('Failed to switch Brahma Console model:', error);
    }
  }
}

/** Read the persisted last-chat id, tolerating a storage-less environment. */
function readStoredChatId(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY_LAST_CHAT) || null;
  } catch {
    return null;
  }
}
