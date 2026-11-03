/**
 * The Brahma Console state service (port of v4
 * `components/providers/brahma-console-provider.tsx` at `236c04e35`).
 *
 * The Brahma Console is a character-less, memory-free generic-LLM surface, so —
 * unlike a Help Chat — there is NO character/eligibility selection and NO
 * pathname tracking (the console is not page-aware). What it adds is the active
 * connection profile (model), switchable at any time, continuing the same chat.
 *
 * A `providedIn: 'root'` singleton so the rail entry, the launcher (which flip
 * `isOpen`) and the floating dialog share one state — exactly v4's app-level
 * provider. Since v4 `99463fc56` it holds NO conversation or model state: each
 * surface (the floating dialog, the workspace tab) has its own
 * {@link import('./brahma-console-session').BrahmaConsoleSession}.
 *
 * @module brahma/brahma-console.service
 */

import { Injectable, computed, inject, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { ConnectionProfileDto } from '../core/core-contract';

/** A connection profile as the model picker needs it (v4 `BrahmaConnectionProfile`). */
export interface BrahmaConnectionProfile {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  /** v4 `99463fc56` — the launcher's picker starts on the default profile. */
  isDefault: boolean;
}

/**
 * The picker's shape — a pure map over the raw rows, applied on READ. The rows
 * themselves are what the shared cache entry holds (P4.116): this service is
 * `providedIn: 'root'`, so its observer outlives every screen, and any refetch
 * it runs rewrites the ONE entry — a mapped `queryFn` here left the Settings
 * cards reading rows with no `isDangerousCompatible` / `supportsImageUpload`.
 */
export function mapBrahmaConnectionProfiles(
  profiles: readonly ConnectionProfileDto[],
): BrahmaConnectionProfile[] {
  return profiles.map((p) => ({
    id: p.id,
    name: p.name || '',
    provider: p.provider || '',
    modelName: p.modelName || '',
    isDefault: p.isDefault === true,
  }));
}

@Injectable({ providedIn: 'root' })
export class BrahmaConsoleService {
  private readonly core = inject(CoreClient);

  /** Whether the floating console dialog is open (v4 `isOpen`). */
  readonly isOpen = signal(false);

  /**
   * The user's connection profiles, shared with the settings surface via the
   * `['connection-profiles']` query key (dedups; v4's one spelling). The entry holds the RAW rows;
   * {@link profiles} maps them to the picker's shape.
   */
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'],
    queryFn: async (): Promise<ConnectionProfileDto[]> => {
      const resp = await this.core.dispatchExpect(
        { type: 'connectionProfileList' },
        'connectionProfiles',
      );
      return resp.data.profiles;
    },
  }));

  /** All of the user's connection profiles (for the model picker). */
  readonly profiles = computed<BrahmaConnectionProfile[]>(() =>
    mapBrahmaConnectionProfiles(this.profilesQuery.data() ?? []),
  );

  /** Whether the profile list is still loading. */
  readonly profilesLoading = computed(() => this.profilesQuery.isLoading());

  /** Whether the console can be opened at all — ≥1 connection profile (v4 L137). */
  readonly isEligible = computed(() => this.profiles().length > 0);

  /**
   * Show the floating console. Since `99463fc56` it no longer resets the
   * conversation: each surface's session (`brahma-console-session.ts`) keeps
   * its own, and reopening returns to it.
   */
  openConsole(): void {
    this.isOpen.set(true);
  }

  /** Hide the floating console (its session — and any reply streaming — survives). */
  closeConsole(): void {
    this.isOpen.set(false);
  }

  /** The launcher button's gesture (v4 `toggleConsole`). */
  toggleConsole(): void {
    this.isOpen.update((open) => !open);
  }
}
