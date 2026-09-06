/**
 * The Help Chat state service (port of v4
 * `components/providers/help-chat-provider.tsx`, baseline `d883a5ee1`).
 *
 * A `providedIn: 'root'` singleton, exactly v4's app-level provider: the rail
 * entry (which flips `isOpen`), the dialog, and the Guide tab all share one
 * state. Where the Brahma console has a model, this has CHARACTERS — the help
 * seats — plus the page the operator is on, which the open chat is re-anchored
 * to as they walk around.
 *
 * Three storage keys, all SPA-only (they never reach the wire), all with v4's
 * exact semantics:
 *
 *  - `quilltap:help-chat-selected-characters` — a JSON array.
 *  - `quilltap:help-chat-last-id` — a PLAIN string, deliberately not JSON. v4
 *    carries a cleanup for values a prior `JSON.stringify` bug double-quoted;
 *    it is kept, because instances written by that build still exist.
 *  - `quilltap:help-tab` (sessionStorage) — owned by the dialog, not here.
 *
 * @module help/help.service
 */

import { DestroyRef, Injectable, computed, effect, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { NavigationEnd, Router } from '@angular/router';
import { injectQuery } from '@tanstack/angular-query-experimental';
import { filter } from 'rxjs';

import { HelpApi, type HelpEligibleCharacter } from './help-wire';

/** localStorage key for the picked help seats (v4 `STORAGE_KEY_SELECTED`). */
export const STORAGE_KEY_SELECTED = 'quilltap:help-chat-selected-characters';
/** localStorage key for the last-open help chat (v4 `STORAGE_KEY_LAST_CHAT`). */
export const STORAGE_KEY_LAST_CHAT = 'quilltap:help-chat-last-id';

/** The eligibility query key — v4 `queryKeys.helpChat.eligibility`. */
export const HELP_ELIGIBILITY_QUERY_KEY = ['help-chat', 'eligibility'] as const;

@Injectable({ providedIn: 'root' })
export class HelpService {
  private readonly api = inject(HelpApi);
  private readonly router = inject(Router);
  private readonly destroyRef = inject(DestroyRef);

  /** Whether the help dialog is open (v4 `isOpen`). */
  readonly isOpen = signal(false);

  /** The active help chat id — persisted to localStorage (v4 `currentChatId`). */
  readonly currentChatId = signal<string | null>(readStoredChatId());

  /** The picked help seats for a NEW chat (v4 `selectedCharacterIds`). */
  readonly selectedCharacterIds = signal<string[]>(loadStorageArray(STORAGE_KEY_SELECTED));

  /** The path the operator is on — the chat's page context (v4 `currentPageUrl`). */
  readonly currentPageUrl = signal(currentPath(this.router));

  private readonly eligibilityQuery = injectQuery(() => ({
    queryKey: HELP_ELIGIBILITY_QUERY_KEY,
    queryFn: () => this.api.eligibility(),
  }));

  /** Every character the server named, eligible or not (v4 `eligibleCharacters`). */
  readonly eligibleCharacters = computed<HelpEligibleCharacter[]>(
    () => this.eligibilityQuery.data()?.characters ?? [],
  );

  /** Why no character can hold a help chat, when none can (v4 `reasons`). */
  readonly reasons = computed<string[]>(() => this.eligibilityQuery.data()?.reasons ?? []);

  /** Whether eligibility is still in flight (v4 `eligibilityLoading`). */
  readonly eligibilityLoading = computed(() => this.eligibilityQuery.isPending());

  /**
   * Whether ANY character can actually hold a help chat.
   *
   * v4 derives this locally — `eligibleCharacters.some(c => c.hasToolCapableProfile)`
   * — rather than reading the payload's own `eligible` flag, and so does this:
   * the list is what the launcher renders, so a disagreement between the flag
   * and the list would show as an enabled button over an empty picker.
   */
  readonly isEligible = computed(() => this.eligibleCharacters().some((c) => c.hasToolCapableProfile));

  /** The subset the launcher offers and `create` falls back to. */
  readonly toolCapableCharacters = computed(() =>
    this.eligibleCharacters().filter((c) => c.hasToolCapableProfile),
  );

  constructor() {
    // v4 auto-selects the FIRST tool-capable character when nothing is picked
    // (`help-chat-provider.tsx:126-131`), so the composer is usable the moment
    // the dialog opens rather than demanding a pick first. v4 runs this in the
    // same effect that syncs the query data — an effect whose deps include
    // `selectedCharacterIds.length` (`:140`), so deselecting the LAST seat
    // re-fires it and the first tool-capable seat snaps back. Tracking the
    // selection here reproduces that (the §3 review of the `p4.9i2` unification).
    effect(() => {
      const eligible = this.toolCapableCharacters();
      if (eligible.length === 0) return;
      if (this.selectedCharacterIds().length > 0) return;
      const autoSelected = [eligible[0].id];
      this.selectedCharacterIds.set(autoSelected);
      saveStorageValue(STORAGE_KEY_SELECTED, autoSelected);
    });

    // Track route changes for context updates (v4's pathname effect). The
    // PATCH fires only while the dialog is OPEN with a chat in hand — walking
    // around with it closed re-anchors nothing.
    this.router.events
      .pipe(
        filter((e): e is NavigationEnd => e instanceof NavigationEnd),
        takeUntilDestroyed(this.destroyRef),
      )
      .subscribe((e) => {
        // Path only — see `currentPath` for why, and for what it costs.
        const next = (e.urlAfterRedirects || e.url).split('?')[0];
        if (next === this.currentPageUrl()) return;
        this.currentPageUrl.set(next);
        const chatId = this.currentChatId();
        if (this.isOpen() && chatId) {
          void this.api.chatUpdateContext(chatId, next).catch((error) => {
            console.error('Failed to update help chat context:', error);
          });
        }
      });
  }

  /** Open the dialog to the LAUNCHER so past chats are visible (v4 `openHelpChat`). */
  openHelpChat(): void {
    // v4 always resets currentChatId — the launcher comes first, every time.
    this.setCurrentChatId(null);
    this.isOpen.set(true);
  }

  /** Close the dialog (v4 `closeHelpChat`). */
  closeHelpChat(): void {
    this.isOpen.set(false);
  }

  /** Set the active chat id and persist it as a PLAIN string (v4). */
  setCurrentChatId(id: string | null): void {
    this.currentChatId.set(id);
    try {
      // Stored plain — not JSON-stringified — since the read takes it directly.
      if (id) localStorage.setItem(STORAGE_KEY_LAST_CHAT, id);
      else localStorage.removeItem(STORAGE_KEY_LAST_CHAT);
    } catch {
      /* ignore */
    }
  }

  /** Add or remove a help seat from the selection (v4 `toggleCharacter`). */
  toggleCharacter(characterId: string): void {
    const prev = this.selectedCharacterIds();
    const next = prev.includes(characterId)
      ? prev.filter((id) => id !== characterId)
      : [...prev, characterId];
    this.selectedCharacterIds.set(next);
    saveStorageValue(STORAGE_KEY_SELECTED, next);
  }

  /** Re-read eligibility (v4 `refreshEligibility`). */
  async refreshEligibility(): Promise<void> {
    await this.eligibilityQuery.refetch();
  }
}

/** v4 `loadStorageArray` — a bad value yields an empty selection, never a throw. */
function loadStorageArray(key: string): string[] {
  try {
    const val = localStorage.getItem(key);
    if (val) {
      const parsed: unknown = JSON.parse(val);
      // v4 returns `JSON.parse(val)` unguarded; guarding the type here cannot
      // change a well-formed read and keeps a hand-edited string out of the
      // signal (which Angular would then render as characters).
      if (Array.isArray(parsed)) return parsed as string[];
    }
  } catch {
    /* ignore */
  }
  return [];
}

function saveStorageValue(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* ignore */
  }
}

/**
 * v4's lazy initialiser for `currentChatId`, cleanup included: a prior build
 * wrote this key through `JSON.stringify`, leaving `"abc"` on disk. Strip the
 * quotes and REWRITE the key, so the repair happens once per instance.
 */
function readStoredChatId(): string | null {
  try {
    let val = localStorage.getItem(STORAGE_KEY_LAST_CHAT) || null;
    // Clean up legacy double-quoted values from prior JSON.stringify bug
    if (val && val.startsWith('"') && val.endsWith('"')) {
      val = val.slice(1, -1);
      localStorage.setItem(STORAGE_KEY_LAST_CHAT, val);
    }
    return val;
  } catch {
    return null;
  }
}

/**
 * The path at construction — the router's own events carry it from then on.
 *
 * PATH ONLY, no query string, because v4 reads `usePathname()` and Next's own
 * typings state the contract outright: `usePathname() // returns "/dashboard"
 * on /dashboard?foo=bar`. The port has already established this mapping once
 * (`documents/open-document-from-search.ts:59`). Read off the ROUTER rather than
 * `window.location`, so the seed and the subsequent `NavigationEnd` reads come
 * from the same source.
 *
 * ⚠ **A v4 finding, reproduced deliberately.** Because `currentPageUrl` is the
 * bare path, the seven `?tab=` rows of `URL_CATEGORY_MAP` can never match
 * from `getCategoryForUrl(currentPageUrl)` — every settings page falls through
 * to bare `/settings` → `settings-system`, so the Guide never auto-expands
 * Appearance, Commonplace Book, Content Routing or AI Providers from the
 * settings screen. v4's `/settings` page reads `tab` from search params, which
 * `usePathname()` drops. This is a candidate upstream filing, NOT a v5 fix: the
 * fix belongs in v4 (pass the search string, or read `useSearchParams`), and
 * porting a unilateral repair would make v5's Guide behave differently from the
 * oracle. Pinned by a spec so the behaviour cannot drift silently either way.
 */
function currentPath(router: Router): string {
  try {
    return router.url.split('?')[0] || '/';
  } catch {
    return '/';
  }
}
