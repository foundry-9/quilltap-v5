import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/item-browser.oracle.ndjson';
import { fixedMenuPlacement } from '../ui/fixed-menu';
import { pluralize } from './folder-browser-state';
import { childFolderRows, tierLabel, tierRows, type BrowserOrigin, type BrowserViewItem } from './folder-view';
import { presentEntries, type MenuEntryLike } from './item-action-menu';

/**
 * The tier-1 SPA differential for the `BROWSER` commit's pure half (P4.D386
 * item 2): v4's REAL `lib/folder-browser/view.ts` (`tierLabel`,
 * `childFolderRows`, `tierRows`), `hooks/useFixedMenu.ts`'s
 * `fixedMenuPlacement`, `components/item-browser/ItemActionMenu.tsx`'s
 * `presentEntries` and `useFolderBrowserState.ts`'s `pluralize`, at the pin
 * `624d0d25c`. Recorded by `apps/web/oracle/item-browser.recorder.ts` (its
 * header has the pinned-worktree invocation); replayed here through v5's
 * `item-browser/folder-view.ts`, `ui/fixed-menu.ts`, `item-action-menu.ts` and
 * `folder-browser-state.ts`.
 *
 * v4's placement reads `window` and the anchor's rect; v5's is DOM-free (the
 * R-15 shape), so a row's `[rect, innerWidth, innerHeight, room]` is passed
 * straight in.
 */

interface Row {
  fn: 'tierLabel' | 'childFolderRows' | 'tierRows' | 'fixedMenuPlacement' | 'presentEntries' | 'pluralize';
  id: string;
  args: unknown[];
  out: unknown;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const isUndef = (v: unknown): boolean => v !== null && typeof v === 'object' && '$undefined' in (v as object);
const revive = (v: unknown): unknown => (isUndef(v) ? undefined : v);

/** The recorder's fixed tierOf: own = no origin, else `scope:id`. */
type TierItem = BrowserViewItem & { id: string };
const tierKey = (i: TierItem): string => (i.origin ? `${i.origin.scope}:${i.origin.id ?? ''}` : 'own');

type EntryShape = { key: string; label: string; disabled: boolean | null; destructive: boolean | null } | false | null | { $undefined: true };
const toEntries = (shapes: EntryShape[]): MenuEntryLike[] =>
  shapes.map((s) => {
    if (isUndef(s)) return undefined;
    if (s === false || s === null) return s;
    const e = s as { key: string; label: string; disabled: boolean | null; destructive: boolean | null };
    return {
      key: e.key,
      label: e.label,
      onSelect: () => {},
      ...(e.disabled === null ? {} : { disabled: e.disabled }),
      ...(e.destructive === null ? {} : { destructive: e.destructive }),
    };
  });
const fromEntries = (entries: MenuEntryLike[]): EntryShape[] =>
  entries.map((e) =>
    e === undefined
      ? { $undefined: true as const }
      : e === false || e === null
        ? e
        : { key: e.key, label: e.label, disabled: e.disabled ?? null, destructive: e.destructive ?? null },
  );

function replay(row: Row): unknown {
  const a = row.args;
  switch (row.fn) {
    case 'tierLabel': {
      const origin = revive(a[0]) as BrowserOrigin | null | undefined;
      return a.length > 1 ? tierLabel(origin, a[1] as string) : tierLabel(origin);
    }
    case 'childFolderRows': {
      const tree = revive(a[2]) as Array<{ path: string; itemCount: number }> | undefined;
      return tree === undefined
        ? childFolderRows(a[0] as string, a[1] as BrowserViewItem[])
        : childFolderRows(a[0] as string, a[1] as BrowserViewItem[], tree);
    }
    case 'tierRows':
      return tierRows(a[0] as TierItem[], tierKey);
    case 'fixedMenuPlacement': {
      const room = revive(a[3]) as number | undefined;
      const rect = a[0] as { top: number; bottom: number; right: number };
      const out =
        room === undefined
          ? fixedMenuPlacement(rect, a[1] as number, a[2] as number)
          : fixedMenuPlacement(rect, a[1] as number, a[2] as number, room);
      return out;
    }
    case 'presentEntries':
      return fromEntries(presentEntries(toEntries(a[0] as EntryShape[])));
    case 'pluralize':
      return pluralize(a[0] as number, a[1] as string);
  }
}

describe("the shared item browser's pure half agrees with v4 at 624d0d25c row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ROWS).toHaveLength(64);
    const count = (fn: Row['fn']) => ROWS.filter((r) => r.fn === fn).length;
    expect(count('tierLabel')).toBe(9);
    expect(count('childFolderRows')).toBe(15);
    expect(count('tierRows')).toBe(8);
    expect(count('fixedMenuPlacement')).toBe(22);
    expect(count('presentEntries')).toBe(6);
    expect(count('pluralize')).toBe(4);
  });

  it('pins the measured byte shift: an empty-named character origin labels `General`', () => {
    const row = ROWS.find((r) => r.fn === 'tierLabel' && r.id.startsWith('character empty name (the byte shift)'))!;
    expect(row.out).toBe('General');
  });

  it.each(ROWS)('$fn — $id', (row) => {
    // JSON drops `undefined` keys, so compare through the same serialisation.
    expect(JSON.parse(JSON.stringify(replay(row)))).toEqual(row.out);
  });
});
