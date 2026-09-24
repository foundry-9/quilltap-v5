/**
 * Where the `@` typeahead gets its characters — the seam between the
 * ProseMirror plugin (framework-free) and the host's query cache.
 *
 * v4's `MentionTypeaheadPlugin` calls `useQuery({ queryKey:
 * queryKeys.characters.list(), enabled: query !== null })` itself, "same key
 * and URL as the spellcheck dictionary feed, so the list is shared". A
 * ProseMirror plugin cannot inject, so the host hands it a
 * {@link MentionCharacterSource} built over the SAME cache entry the other
 * `characterKeys.list()` readers use; {@link createQueryMentionSource} is that
 * builder, a thin `QueryObserver` wrapper that keeps TanStack's own
 * `isPending` / `isError` / `data` semantics (and the app client's `retry: 1`,
 * which is v4's too).
 *
 * @module editor/mentions/mention-source
 */

import { QueryObserver, type QueryClient, type QueryKey } from '@tanstack/angular-query-experimental';

import type { MentionCandidate } from '../../chat/mentions/mention-typeahead';

/** What the plugin reads — v4's `{ data, isPending, isError }`. */
export interface MentionCharacterSnapshot {
  /** The list, once one has arrived; `undefined` before. */
  characters: readonly MentionCandidate[] | undefined;
  isPending: boolean;
  isError: boolean;
}

export interface MentionCharacterSource {
  snapshot(): MentionCharacterSnapshot;
  /**
   * v4's `enabled: query !== null` — the list is fetched only once a trigger is
   * live, so a writer who never types `@` never pays for it.
   */
  setActive(active: boolean): void;
  /** Called whenever the snapshot may have changed. Returns the unsubscribe. */
  subscribe(listener: () => void): () => void;
}

/**
 * A source over one query-cache entry. `queryFn` must store exactly what every
 * other reader of `queryKey` stores (the raw list — never a mapped shape), or
 * the shared entry would change type under them.
 */
export function createQueryMentionSource(
  queryClient: QueryClient,
  queryKey: QueryKey,
  queryFn: () => Promise<readonly MentionCandidate[]>,
): MentionCharacterSource {
  let active = false;
  const observer = new QueryObserver(queryClient, { queryKey, queryFn, enabled: false });

  return {
    snapshot() {
      const result = observer.getCurrentResult();
      return {
        characters: result.data as readonly MentionCandidate[] | undefined,
        isPending: result.isPending,
        isError: result.isError,
      };
    },
    setActive(next: boolean) {
      if (next === active) return;
      active = next;
      observer.setOptions({ queryKey, queryFn, enabled: next });
    },
    subscribe(listener: () => void) {
      return observer.subscribe(() => listener());
    },
  };
}
