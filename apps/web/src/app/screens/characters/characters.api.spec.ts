import { describe, expect, it } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import {
  archiveCharacter,
  characterKeys,
  countArchiveBundles,
  deleteArchiveBundle,
  fetchCharacterList,
  rehydrateCharacter,
} from './characters.api';

/** A `CoreClient` stub recording every dispatched request. */
function recorder(reply: Record<string, unknown> | (() => never) = {}): {
  client: Partial<CoreClient>;
  seen: Array<Record<string, unknown>>;
} {
  const seen: Array<Record<string, unknown>> = [];
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: Record<string, unknown>) => {
      seen.push(req);
      if (typeof reply === 'function') reply();
      return reply;
    }) as CoreClient['dispatchData'],
  };
  return { client, seen };
}

describe('characterKeys.list — the archived-filter cache keys (P4.D64)', () => {
  it('keys the archived variant DISTINCTLY from the unfiltered list', () => {
    // v4 `AuroraView.tsx:47-49`: "the two filter states cache under distinct
    // keys so they never cross-contaminate". The mutation-killer here is
    // key EQUALITY: a key builder that dropped the filter object would make
    // both fetches share one cache entry, and toggling would show stale rows.
    const plain = JSON.stringify(characterKeys.list());
    const archived = JSON.stringify(characterKeys.list({ archived: 'include' }));
    expect(plain).not.toEqual(archived);
    expect(archived).toContain('include');
  });

  it('keeps the archived key distinct from the other filter variants', () => {
    const byControl = JSON.stringify(characterKeys.list({ controlledBy: 'user' }));
    const archived = JSON.stringify(characterKeys.list({ archived: 'include' }));
    expect(byControl).not.toEqual(archived);
  });

  it('nests both list keys under the `all` prefix so one invalidation reaches both', () => {
    // v4 `:72-73` widened mutation invalidation from the list key to the
    // `characters` PREFIX "so the other archived-filter variant refreshes too".
    // Prefix matching is positional, so the guarantee is that both keys START
    // with `characterKeys.all`.
    for (const key of [characterKeys.list(), characterKeys.list({ archived: 'include' })]) {
      expect(key.slice(0, characterKeys.all.length)).toEqual([...characterKeys.all]);
    }
  });
});

describe('fetchCharacterList — the archived filter on the wire', () => {
  it('sends `archived: include` only when asked, and omits the param otherwise', async () => {
    const { client, seen } = recorder({ characters: [] });
    await fetchCharacterList(client as CoreClient, { archived: 'include' });
    await fetchCharacterList(client as CoreClient);
    expect(seen[0]).toEqual({ type: 'characterList', archived: 'include' });
    // v4 fetches the bare `/api/v1/characters` when the toggle is off — the
    // absent param IS the exclude default (§1), never an explicit 'exclude'.
    expect(seen[1]).toEqual({ type: 'characterList' });
  });
});

describe('the archive verbs (P4.D64 §2)', () => {
  it('archiveCharacter dispatches `characterArchive` with the id', async () => {
    const { client, seen } = recorder({ archived: true, archiveFileId: 'f1', pruneComplete: true });
    const result = await archiveCharacter(client as CoreClient, 'char-1');
    expect(seen[0]).toEqual({ type: 'characterArchive', characterId: 'char-1' });
    expect(result.pruneComplete).toBe(true);
    expect(result.archiveFileId).toBe('f1');
  });

  it('rehydrateCharacter dispatches `characterRehydrate` and carries the restored counts', async () => {
    const { client, seen } = recorder({
      rehydrated: true,
      archived: false,
      archiveBundleFileId: 'bundle-9',
      restored: { memories: 3, documents: 2, blobs: 1 },
      warnings: [],
    });
    const result = await rehydrateCharacter(client as CoreClient, 'char-1');
    expect(seen[0]).toEqual({ type: 'characterRehydrate', characterId: 'char-1' });
    expect(result.restored).toEqual({ memories: 3, documents: 2, blobs: 1 });
    expect(result.archiveBundleFileId).toBe('bundle-9');
  });

  it('deleteArchiveBundle sends a bare fileDelete — no force, no dissociate', async () => {
    const { client, seen } = recorder({});
    await deleteArchiveBundle(client as CoreClient, 'bundle-9');
    // v4 `RehydrateBundleDialog.tsx:31` sends a plain DELETE with no body; a
    // bundle has no associations, so neither option belongs on the wire.
    expect(seen[0]).toEqual({ type: 'fileDelete', fileId: 'bundle-9' });
  });
});

describe('countArchiveBundles — the courtesy count (P4.D64 §3)', () => {
  it('asks for the ARCHIVE category and reports the row count', async () => {
    const { client, seen } = recorder({ files: [{ id: 'a' }, { id: 'b' }] });
    expect(await countArchiveBundles(client as CoreClient)).toBe(2);
    expect(seen[0]).toEqual({ type: 'filesList', category: 'ARCHIVE' });
  });

  it('swallows a failure and reports null — the count never blocks the card', async () => {
    // v4 `:29-31`: "count is a courtesy; the change itself reports the real
    // numbers". A rejected read must not surface an error to the operator.
    const { client } = recorder(() => {
      throw new Error('files route not available');
    });
    await expect(countArchiveBundles(client as CoreClient)).resolves.toBeNull();
  });

  it('reports null when the body carries no files array', async () => {
    const { client } = recorder({ notFiles: true });
    expect(await countArchiveBundles(client as CoreClient)).toBeNull();
  });
});
