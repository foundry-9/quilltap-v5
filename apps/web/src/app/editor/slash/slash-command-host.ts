/**
 * What the composer hands the `/` command menu — v4
 * `components/chat/lexical/slash-commands/context.ts` (`4877fc2da`), plus the
 * two lists v4's plugin fetches for itself.
 *
 * v4's `SlashCommandMenu` runs two `useQuery`s of its own (Prospero's tools on
 * `queryKeys.tools.byChat`, Pascal's roster on `queryKeys.customTools.byChat`),
 * each `enabled` only while the menu is open. A ProseMirror plugin cannot
 * inject, so — as the `@` typeahead does with `mention-source.ts` — the host
 * hands it two {@link SlashToolSource}s built over the SAME cache entries the
 * other readers use: the Run Tool modal's `['tools', chatId, 'schemas']` (it
 * already issues exactly v4's `?chatId=…&includeSchemas=true` read, so a `/`
 * warms the modal and the modal warms the `/`) and the custom-tool popup's
 * `customToolsKeys.byChat(chatId)` (v4: "Same key as the custom-tool dialog and
 * the Salon's gutter gate").
 *
 * This directory never imports the core client: the decisions live in
 * `chat/slash-commands/`, the transport in `chat/` (the host the composer
 * assembles, `chat/slash-command-runner.ts`); the adapter sees only this
 * contract.
 *
 * @module editor/slash/slash-command-host
 */

import { QueryObserver, type QueryClient, type QueryKey } from '@tanstack/angular-query-experimental';

import type {
  ComposerAction,
  SlashBuiltInToolSource,
  SlashCustomToolSource,
} from '../../chat/slash-commands';

/** A custom tool as the slash menu names it to the dialog or the run route. */
export interface SlashCustomToolRef {
  name: string;
  asCharacterId: string | null;
}

/** What the plugin reads — v4's `{ data, isPending, isError }`, mapped to the list. */
export interface SlashToolSnapshot<T> {
  /** The list, once one has arrived; `undefined` before. */
  tools: readonly T[] | undefined;
  isPending: boolean;
  isError: boolean;
}

/** One query-backed list — the `MentionCharacterSource` shape. */
export interface SlashToolSource<T> {
  snapshot(): SlashToolSnapshot<T>;
  /**
   * v4's `enabled: menuOpen` — fetched only while the menu is open, so a
   * composer that never sees a `/` never asks.
   */
  setActive(active: boolean): void;
  /** Called whenever the snapshot may have changed. Returns the unsubscribe. */
  subscribe(listener: () => void): () => void;
}

/** v4 `SlashCommandContext`, plus the two lists. */
export interface SlashCommandHost {
  chatId: string;
  /**
   * What the composer wired. `tool`, `pascal` and `roll` are derived from the
   * dialog openers below and need not appear here.
   */
  actions: Partial<Record<ComposerAction, () => void>>;
  /** Open Run Tool — on a tool's form when given an id. */
  openRunTool(toolId?: string): void;
  /** Open Pascal's table — on a tool's form when given one. */
  openCustomTool(initial?: SlashCustomToolRef): void;
  /** Run a built-in tool that declares no parameters, at once. */
  runBuiltInNow(toolId: string): Promise<void>;
  /** Run a custom tool that declares no parameters, at once. */
  runCustomNow(tool: SlashCustomToolRef): Promise<void>;
  /**
   * Whether the chat resolves a Pascal roster (`/pascal` and the customs are
   * listed only then). Read live — v5's ONE derivation is the custom-tool
   * popup's `available()`, the wand's own gate.
   */
  customToolsAvailable(): boolean;
  /** Prospero's tools for this chat (`toolsList`, with schemas). */
  builtInTools: SlashToolSource<SlashBuiltInToolSource>;
  /** Pascal's roster for this chat (`chatCustomToolsList`). */
  customTools: SlashToolSource<SlashCustomToolSource>;
}

export interface SlashToolSourceOptions<R, T> {
  queryKey: QueryKey;
  /**
   * Must store exactly what every other reader of `queryKey` stores (the raw
   * reply — never a mapped shape), or the shared entry would change type under
   * them; {@link select} maps it for the menu.
   */
  queryFn: () => Promise<R>;
  select: (data: R) => readonly T[] | undefined;
  /**
   * v4's per-list `staleTime`: `60_000` for Prospero's tools, `0` for Pascal's
   * roster ("fresh per open … refetched whenever the menu opens"). Activation
   * flips `enabled` false → true, and TanStack refetches a STALE entry on that
   * flip — so `0` refetches on every open and `60_000` serves a warm cache,
   * exactly as v4's `useQuery` does when `menuOpen` turns true.
   */
  staleTime: number;
}

/** A source over one query-cache entry (the `createQueryMentionSource` precedent). */
export function createQuerySlashToolSource<R, T>(
  queryClient: QueryClient,
  options: SlashToolSourceOptions<R, T>,
): SlashToolSource<T> {
  const { queryKey, queryFn, select, staleTime } = options;
  let active = false;
  const observer = new QueryObserver(queryClient, { queryKey, queryFn, staleTime, enabled: false });

  return {
    snapshot() {
      const result = observer.getCurrentResult();
      return {
        tools: result.data === undefined ? undefined : select(result.data as R),
        isPending: result.isPending,
        isError: result.isError,
      };
    },
    setActive(next: boolean) {
      if (next === active) return;
      active = next;
      observer.setOptions({ queryKey, queryFn, staleTime, enabled: next });
    },
    subscribe(listener: () => void) {
      return observer.subscribe(() => listener());
    },
  };
}
