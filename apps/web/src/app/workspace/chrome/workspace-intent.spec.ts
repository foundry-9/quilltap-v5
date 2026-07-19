/**
 * parseOpenIntent — the `?open=` intent parser (v4 `WorkspaceIntent.tsx`).
 */

import { describe, expect, it } from 'vitest';

import { parseOpenIntent } from './workspace-intent';

function reader(q: Record<string, string>) {
  return { get: (n: string) => (n in q ? q[n] : null) };
}

describe('parseOpenIntent', () => {
  it('returns null with no open param', () => {
    expect(parseOpenIntent(reader({}))).toBeNull();
  });

  it('returns null for an unknown / non-openable kind (character-view is not openable)', () => {
    expect(parseOpenIntent(reader({ open: 'nonsense' }))).toBeNull();
    expect(parseOpenIntent(reader({ open: 'character-view', characterId: 'x' }))).toBeNull();
  });

  it('opens a singleton with no payload', () => {
    expect(parseOpenIntent(reader({ open: 'aurora' }))).toEqual({ kind: 'aurora', payload: undefined });
    expect(parseOpenIntent(reader({ open: 'home' }))).toEqual({ kind: 'home', payload: undefined });
  });

  it('opens a salon with its chatId, and skips when it is missing', () => {
    expect(parseOpenIntent(reader({ open: 'salon', chatId: 'c1' }))).toEqual({
      kind: 'salon',
      payload: { chatId: 'c1' },
    });
    expect(parseOpenIntent(reader({ open: 'salon' }))).toBeNull();
  });

  it('carries settings tab/section', () => {
    expect(parseOpenIntent(reader({ open: 'settings', tab: 'system', section: 'memory' }))).toEqual({
      kind: 'settings',
      payload: { tab: 'system', section: 'memory' },
    });
    expect(parseOpenIntent(reader({ open: 'settings' }))).toEqual({
      kind: 'settings',
      payload: { tab: undefined, section: undefined },
    });
  });

  it('carries custom-tools mount/path/new (or no payload)', () => {
    expect(parseOpenIntent(reader({ open: 'custom-tools' }))).toEqual({
      kind: 'custom-tools',
      payload: undefined,
    });
    expect(parseOpenIntent(reader({ open: 'custom-tools', mount: 'm1', path: 'p/a', new: '1' }))).toEqual({
      kind: 'custom-tools',
      payload: { mountPointId: 'm1', path: 'p/a', create: true },
    });
  });

  it('carries wardrobe characterId (optional)', () => {
    expect(parseOpenIntent(reader({ open: 'wardrobe', characterId: 'x' }))).toEqual({
      kind: 'wardrobe',
      payload: { characterId: 'x' },
    });
    expect(parseOpenIntent(reader({ open: 'wardrobe' }))).toEqual({
      kind: 'wardrobe',
      payload: undefined,
    });
  });

  it('opens character-edit with its id, and skips when it is missing', () => {
    expect(parseOpenIntent(reader({ open: 'character-edit', characterId: 'x', tab: 't' }))).toEqual({
      kind: 'character-edit',
      payload: { characterId: 'x', tab: 't' },
    });
    expect(parseOpenIntent(reader({ open: 'character-edit' }))).toBeNull();
  });

  it('builds a document-standalone payload with a docKey derived from identity', () => {
    const out = parseOpenIntent(
      reader({ open: 'document-standalone', scope: 'document_store', mountPoint: 'm', filePath: 'notes.md' }),
    );
    expect(out).toEqual({
      kind: 'document-standalone',
      payload: {
        docKey: 'document_store:m:notes.md',
        scope: 'document_store',
        mountPoint: 'm',
        filePath: 'notes.md',
        targetFolder: undefined,
      },
    });
  });
});
