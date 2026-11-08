import { computed, inject, signal, type Signal } from '@angular/core';

import { CoreClient } from '../core/core-client';

/**
 * Every container a tiered item can live in — the characters, projects and
 * groups lists, sorted by name — for the wardrobe's and the Inventory's
 * container selects and their Move / Copy to another tier dialogs. (Quilltap
 * General is always there and needs no read.) A port of v4
 * `lib/hooks/use-container-catalogue.ts` at the pin `624d0d25c` (NEW at
 * `b2b783b47`).
 *
 * v5 shape: v4's three TanStack queries over `/api/v1/characters`,
 * `/projects`, `/groups` are the dispatch LIST verbs `characterList` /
 * `projectList` / `groupList`, read once per catalogue, each failing soft to
 * an empty list (the `inventory-control-dialog.ts` precedent) — a failed
 * characters read still SETTLES (v4's `isFetched` is true after an error), so
 * auto-selection never waits forever.
 *
 * @module item-browser/container-catalogue
 */

export interface CatalogueEntry {
  id: string;
  name: string;
}

export interface CatalogueCharacter extends CatalogueEntry {
  avatarUrl?: string | null;
}

export interface ContainerCatalogue {
  readonly characters: Signal<CatalogueCharacter[]>;
  readonly projects: Signal<CatalogueEntry[]>;
  readonly groups: Signal<CatalogueEntry[]>;
  /** The characters read has settled (successfully or not) — auto-selection waits for it. */
  readonly charactersSettled: Signal<boolean>;
  /** Any of the three reads is still loading. */
  readonly loading: Signal<boolean>;
}

/** v4 `byName` — a plain `localeCompare`, no options. */
const byName = (a: { name: string }, b: { name: string }): number => a.name.localeCompare(b.name);

/** v4 `useContainerCatalogue`. Call in an injection context. */
export function injectContainerCatalogue(): ContainerCatalogue {
  const core = inject(CoreClient);
  const characters = signal<CatalogueCharacter[]>([]);
  const projects = signal<CatalogueEntry[]>([]);
  const groups = signal<CatalogueEntry[]>([]);
  const charactersSettled = signal(false);
  const pending = signal(3);

  const read = (
    type: 'characterList' | 'projectList' | 'groupList',
    key: string,
    apply: (rows: Array<Record<string, unknown>>) => void,
  ): void => {
    core
      .dispatchData({ type })
      .then(
        (data) => apply((data?.[key] as Array<Record<string, unknown>> | undefined) ?? []),
        () => apply([]),
      )
      .finally(() => pending.update((n) => n - 1));
  };

  read('characterList', 'characters', (rows) => {
    characters.set([...(rows as unknown as CatalogueCharacter[])].sort(byName));
    charactersSettled.set(true);
  });
  read('projectList', 'projects', (rows) =>
    projects.set(
      (rows as unknown as CatalogueEntry[]).map((p) => ({ id: p.id, name: p.name || 'Untitled project' })).sort(byName),
    ),
  );
  read('groupList', 'groups', (rows) =>
    groups.set(
      (rows as unknown as CatalogueEntry[]).map((g) => ({ id: g.id, name: g.name || 'Untitled group' })).sort(byName),
    ),
  );

  return {
    characters: characters.asReadonly(),
    projects: projects.asReadonly(),
    groups: groups.asReadonly(),
    charactersSettled: charactersSettled.asReadonly(),
    loading: computed(() => pending() > 0),
  };
}
