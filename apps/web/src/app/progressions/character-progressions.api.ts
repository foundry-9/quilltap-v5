import { computed, inject, signal, type Signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { CharacterDetail } from '../core/core-contract';
import { characterKeys, fetchCharacter } from '../screens/characters/characters.api';
import { parseProgressions } from './engine';
import { PROGRESSIONS_METADATA_KEY, type Progression, type Progressions } from './schema';

/**
 * The Aurora progressions editor's data layer (v4
 * `components/characters/progressions/useCharacterProgressions.ts` at
 * `25f534c0b`), whose header rides across because it records the whole design:
 *
 * > Progressions live under one reserved key inside the character's vault
 * > `metadata.json`, so there is no route of their own to call: reads come off
 * > the hydrated character, and a save is a read-modify-write of that object
 * > through the ordinary character PUT.
 * >
 * > The RMW is the load-bearing part. `PUT /api/v1/characters/[id]` REPLACES
 * > the whole `metadata` object, so every other key the user has out there —
 * > `faction`, `hasAnsibleAccess`, whatever they invented — has to be spread
 * > back in or this editor would quietly eat the rest of their fact sheet.
 *
 * v5 reads and writes through `characterGet` / `characterUpdate` rather than
 * v4's REST URLs (the `characters.api.ts` standing rule), and the server side
 * already carries it: `db/vault_character_write.rs` takes `metadata` as an
 * `Option<Value>` whole-object replace, absent meaning "no opinion". **No new
 * dispatch verb, no new REST edge, no `api/types.rs` variant.**
 *
 * @module progressions/character-progressions.api
 */

/** What {@link injectCharacterProgressions} hands back (v4's hook's return). */
export interface CharacterProgressions {
  /** The character's valid progressions, keyed by id. Malformed entries are dropped. */
  progressions: Signal<Progressions>;
  /** Ids the vault holds that this editor could not parse, so the card can say so. */
  invalidIds: Signal<string[]>;
  /** True while the tombstone rule would refuse the PUT anyway. */
  isArchived: Signal<boolean>;
  isLoading: Signal<boolean>;
  /** Write the whole set. Everything else in metadata survives. */
  save: (next: Progressions) => Promise<void>;
  /** v4 `save.isPending` — the editor's Saving… gate. */
  savePending: Signal<boolean>;
  /** v4 `remove.isPending` — the delete confirm's disabled gate. */
  removePending: Signal<boolean>;
  /** The same write, tracked by the delete confirm's own pending flag. */
  remove: (next: Progressions) => Promise<void>;
}

/**
 * The read-modify-write itself. Re-reads the character rather than trusting a
 * snapshot this component may have been holding since before someone else's
 * write: the PUT replaces the whole object, and a stale spread would drop their
 * keys.
 *
 * The reserved key is DELETED, not emptied, when the last progression goes —
 * `parseProgressions` reads an empty object and an absent key identically, but
 * leaving `"progressions": {}` behind in someone's fact sheet is litter.
 */
async function writeProgressions(
  core: CoreClient,
  characterId: string,
  next: Progressions,
): Promise<void> {
  const fresh = await fetchCharacter(core, characterId);
  const current: unknown = fresh.metadata;
  const metadata =
    typeof current === 'object' && current !== null && !Array.isArray(current)
      ? (current as Record<string, unknown>)
      : {};

  const body: Record<string, unknown> = { ...metadata };
  if (Object.keys(next).length === 0) delete body[PROGRESSIONS_METADATA_KEY];
  else body[PROGRESSIONS_METADATA_KEY] = next;

  await core.dispatchData({
    type: 'characterUpdate',
    characterId,
    character: { metadata: body },
  });
}

/**
 * One character's progressions, read off the hydrated character and written
 * back through it. **Must be called from an injection context** (a component
 * field initializer) — the `injectCharacterSubprompts` precedent.
 *
 * v4 keeps `save` and `remove` as two separate mutations over the same function
 * so their pending flags are independent (the editor's Saving… label must not
 * light up because a delete confirm is in flight, and vice versa); the two
 * signals below are that, without a second query cache entry.
 */
export function injectCharacterProgressions(characterId: () => string): CharacterProgressions {
  const core = inject(CoreClient);
  const queryClient = injectQueryClient();

  const key = computed(() => characterKeys.detail(characterId()));

  const query = injectQuery(() => ({
    queryKey: key(),
    queryFn: () => fetchCharacter(core, characterId()),
  }));

  const savePending = signal(false);
  const removePending = signal(false);

  const character = computed<CharacterDetail | undefined>(() => query.data());

  // v4 collects the invalid ids through `parseProgressions`'s `onIssue` sink on
  // the same pass that produces the survivors, so the two can never disagree
  // about which entries were dropped.
  const parsed = computed(() => {
    const invalidIds: string[] = [];
    const progressions = parseProgressions(character()?.metadata, (id) => {
      invalidIds.push(id);
    });
    return { progressions, invalidIds };
  });

  const write = async (next: Progressions, pending: ReturnType<typeof signal<boolean>>) => {
    pending.set(true);
    try {
      await writeProgressions(core, characterId(), next);
      await queryClient.invalidateQueries({ queryKey: characterKeys.detail(characterId()) });
      await queryClient.invalidateQueries({ queryKey: characterKeys.all });
    } finally {
      pending.set(false);
    }
  };

  return {
    progressions: computed(() => parsed().progressions),
    invalidIds: computed(() => parsed().invalidIds),
    isArchived: computed(() => Boolean(character()?.archivedAt)),
    isLoading: computed(() => query.isLoading()),
    savePending,
    removePending,
    save: (next) => write(next, savePending),
    remove: (next) => write(next, removePending),
  };
}

/**
 * Coerce a display name into a progression id, the way the subprompts editor
 * coerces a title into a filename: lowercase, non-identifier runs collapsed to
 * a hyphen, and a leading letter guaranteed because the pattern demands one.
 */
export function idFromName(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64);
  if (slug === '') return 'progression';
  return /^[a-z]/.test(slug) ? slug : `p-${slug}`.slice(0, 64);
}
