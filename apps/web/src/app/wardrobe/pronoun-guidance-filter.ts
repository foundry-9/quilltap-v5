/**
 * The Wardrobe UI's pronoun-aware filter for one character — a port of v4
 * `lib/hooks/use-pronoun-guidance-filter.ts` (`eb90d9ed8`, at `236c04e35`):
 * their pronouns (read through the character detail query), an on/off switch
 * that starts on, and the predicate the lists and pickers apply. The rule
 * itself is `fitsPronounGuidance` (`wardrobe/pronoun-guidance.ts`), the same
 * one the `wardrobe_list` tool and the "Let character choose" pass use.
 *
 * `applies` is false with no character, or one whose pronouns are unset or
 * custom: nothing would be hidden, so callers need not show the switch.
 *
 * Must be created in an injection context (it injects a query).
 *
 * @module wardrobe/pronoun-guidance-filter
 */

import { computed, signal, type Signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { standardPronounSubjectOf } from '../characters/pronoun-presets';
import type { CoreClient } from '../core/core-client';
import type { Pronouns } from '../core/core-contract';
import { characterKeys, fetchCharacter } from '../screens/characters/characters.api';
import { fitsPronounGuidance } from './pronoun-guidance';

type GuidedItem = { pronounGuidance?: readonly string[] | null };

export interface PronounGuidanceFilter {
  /** The character has standard pronouns, so guidance can hide something. */
  readonly applies: Signal<boolean>;
  /** The switch: on (the default) honours guidance. */
  readonly aware: Signal<boolean>;
  setAware(next: boolean): void;
  /** Is the item shown under the current switch? (Reads signals — call it in a computed.) */
  shows(item: GuidedItem): boolean;
  /** The items shown under the current switch. */
  offer<T extends GuidedItem>(items: readonly T[]): T[];
  /** How many of `items` guidance would hide (whatever the switch says). */
  countHeldBack(items: readonly GuidedItem[]): number;
}

export function createPronounGuidanceFilter(
  characterId: Signal<string | null>,
  core: CoreClient,
): PronounGuidanceFilter {
  const query = injectQuery(() => {
    const id = characterId();
    return {
      queryKey: characterKeys.detail(id ?? 'none'),
      queryFn: () => fetchCharacter(core, id!),
      enabled: Boolean(id),
    };
  });
  const pronouns = computed<Pronouns | null>(() =>
    characterId() ? (query.data()?.pronouns ?? null) : null,
  );
  const applies = computed(() => standardPronounSubjectOf(pronouns()) !== null);
  const aware = signal(true);

  const shows = (item: GuidedItem): boolean => !aware() || fitsPronounGuidance(item, pronouns());
  return {
    applies,
    aware: aware.asReadonly(),
    setAware: (next) => aware.set(next),
    shows,
    offer: (items) => items.filter(shows),
    countHeldBack: (items) => items.filter((i) => !fitsPronounGuidance(i, pronouns())).length,
  };
}
