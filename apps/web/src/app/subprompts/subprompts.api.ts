import { computed, inject, type Signal, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { SubpromptRecord } from '../core/core-contract';
import { RealtimeService } from '../core/realtime.service';
import { characterKeys } from '../screens/characters/characters.api';

/**
 * The subprompts data layer (v4 `components/subprompts/useCharacterSubprompts.ts`,
 * `2f4254b42`): one query for a character's `Subprompts/` folder plus the three
 * writes that invalidate it, shared by the New-Chat picker, the Salon
 * Participants drawer and the Aurora System Prompts tab "so every dropdown that
 * lists subprompts reads the same cache" (v4's own header).
 *
 * Every op reads through {@link CoreClient.dispatchData} against the §C.2 verbs
 * rather than v4's REST URLs — the SPA's standing rule (`characters.api.ts`
 * header). One consequence is recorded rather than ported: v4's
 * `encodeURIComponent(subpromptId)` in `itemUrl` has **no v5 counterpart**,
 * because no URL is ever built — the id rides as a JSON field of
 * `characterSubpromptGet` / `…Update` / `…Delete`.
 *
 * v4's `subpromptErrorMessage(err, fallback)` has no counterpart either, and for
 * a measured reason: its first branch unwraps an `ApiFetchError`'s parsed
 * `{error}` body, but v5's {@link CoreDispatchError} is CONSTRUCTED from the
 * `{type:"error"}` envelope, so `err.message` is already that sentence. What is
 * left — an `Error` yields its message, anything else the caller's fallback — is
 * exactly `coreErrorMessage`, which every subprompts surface calls directly.
 *
 * @module subprompts/subprompts.api
 */

/** §C.2 `characterSubpromptList` — title-sorted; `[]` for no vault / no folder. */
export async function listSubprompts(
  core: CoreClient,
  characterId: string,
): Promise<SubpromptRecord[]> {
  const data = await core.dispatchData({ type: 'characterSubpromptList', characterId });
  return (data['subprompts'] as SubpromptRecord[] | undefined) ?? [];
}

/** §C.2 `characterSubpromptGet`. */
export async function getSubprompt(
  core: CoreClient,
  characterId: string,
  subpromptId: string,
): Promise<SubpromptRecord> {
  const data = await core.dispatchData({ type: 'characterSubpromptGet', characterId, subpromptId });
  return data['subprompt'] as SubpromptRecord;
}

/** §C.2 `characterSubpromptCreate`. */
export async function createSubprompt(
  core: CoreClient,
  characterId: string,
  input: { title: string; content: string },
): Promise<SubpromptRecord> {
  const data = await core.dispatchData({
    type: 'characterSubpromptCreate',
    characterId,
    title: input.title,
    content: input.content,
  });
  return data['subprompt'] as SubpromptRecord;
}

/** §C.2 `characterSubpromptUpdate` — both fields optional, as v4's PUT is. */
export async function updateSubprompt(
  core: CoreClient,
  characterId: string,
  subpromptId: string,
  patch: { title?: string; content?: string },
): Promise<SubpromptRecord> {
  const data = await core.dispatchData({
    type: 'characterSubpromptUpdate',
    characterId,
    subpromptId,
    ...(patch.title !== undefined ? { title: patch.title } : {}),
    ...(patch.content !== undefined ? { content: patch.content } : {}),
  });
  return data['subprompt'] as SubpromptRecord;
}

/** §C.2 `characterSubpromptDelete`. */
export async function deleteSubprompt(
  core: CoreClient,
  characterId: string,
  subpromptId: string,
): Promise<void> {
  await core.dispatchData({ type: 'characterSubpromptDelete', characterId, subpromptId });
}

/** What {@link injectCharacterSubprompts} hands back (v4's hook's return). */
export interface CharacterSubprompts {
  /** The character's subprompts, `[]` until the query answers. */
  subprompts: Signal<SubpromptRecord[]>;
  /** v4 `enabled && query.isLoading` — a deferred read never reads as loading. */
  isLoading: Signal<boolean>;
  error: Signal<unknown>;
  refetch: () => void;
  create: (input: { title: string; content: string }) => Promise<SubpromptRecord>;
  update: (
    subpromptId: string,
    patch: { title?: string; content?: string },
  ) => Promise<SubpromptRecord>;
  remove: (subpromptId: string) => Promise<void>;
  /** v4 `create.isPending || update.isPending` — the editor's Saving… gate. */
  savePending: Signal<boolean>;
  /** v4 `remove.isPending` — the delete confirm's disabled gate. */
  removePending: Signal<boolean>;
}

/**
 * List + writes for one character's subprompts. **Must be called from an
 * injection context** (a component field initializer) — the `injectCharInsert
 * Settings` precedent.
 *
 * `enabled` is v4's option of the same name, and it matters: the picker defers
 * the read until it is opened or already carries a selection, and the editor
 * modal passes `enabled: false` because it only ever writes.
 *
 * The fallback poll is v4's `useRealtimeRefetchInterval(60_000)` — the P4.D125
 * gating, so a live channel polls nothing and a dropped one degrades to the
 * pre-realtime cadence.
 */
export function injectCharacterSubprompts(
  characterId: () => string | null | undefined,
  options: { enabled?: () => boolean } = {},
): CharacterSubprompts {
  const core = inject(CoreClient);
  const queryClient = injectQueryClient();
  const realtime = inject(RealtimeService);

  const enabled = computed(() => !!characterId() && (options.enabled?.() ?? true));
  const key = computed(() => characterKeys.subprompts(characterId() ?? ''));

  const query = injectQuery(() => ({
    queryKey: key(),
    queryFn: () => listSubprompts(core, characterId()!),
    enabled: enabled(),
    refetchInterval: realtime.refetchInterval(60_000),
  }));

  const savePending = signal(false);
  const removePending = signal(false);

  const invalidate = () => queryClient.invalidateQueries({ queryKey: key() });

  return {
    subprompts: computed(() => query.data() ?? []),
    isLoading: computed(() => enabled() && query.isLoading()),
    error: computed(() => query.error()),
    refetch: () => void query.refetch(),
    savePending,
    removePending,
    async create(input) {
      savePending.set(true);
      try {
        const saved = await createSubprompt(core, characterId()!, input);
        await invalidate();
        return saved;
      } finally {
        savePending.set(false);
      }
    },
    async update(subpromptId, patch) {
      savePending.set(true);
      try {
        const saved = await updateSubprompt(core, characterId()!, subpromptId, patch);
        await invalidate();
        return saved;
      } finally {
        savePending.set(false);
      }
    },
    async remove(subpromptId) {
      removePending.set(true);
      try {
        await deleteSubprompt(core, characterId()!, subpromptId);
        await invalidate();
      } finally {
        removePending.set(false);
      }
    },
  };
}
