/**
 * parseOpenIntent — the `?open=` intent parser (v4 `WorkspaceIntent.tsx`).
 *
 * The bottom section replays v4's own `8d86847a` test delta
 * (`__tests__/unit/components/workspace/workspace-intent.test.tsx`, +73) case
 * for case, translated to the v5 harness: v4 renders `<WorkspaceIntent/>` inside
 * a provider and probes the resulting tab set, so v5 drives the real
 * `WorkspaceService` through `parseOpenIntent` + `applyOpenIntent` and probes
 * the same way.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import { WorkspaceService } from '../workspace.service';
import { applyOpenIntent, parseOpenIntent } from './workspace-intent';

function reader(q: Record<string, string>) {
  return { get: (n: string) => (n in q ? q[n] : null) };
}

describe('parseOpenIntent', () => {
  it('returns null with no open param', () => {
    expect(parseOpenIntent(reader({}))).toBeNull();
  });

  it('returns null for an unknown / non-openable kind', () => {
    expect(parseOpenIntent(reader({ open: 'nonsense' }))).toBeNull();
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

  // --- v4 `8d86847a`: character-view joins the openable set --------------
  it('opens character-view with its id + tab, and skips when the id is missing', () => {
    expect(
      parseOpenIntent(reader({ open: 'character-view', characterId: 'abc', tab: 'conversations' })),
    ).toEqual({ kind: 'character-view', payload: { characterId: 'abc', tab: 'conversations' } });
    expect(parseOpenIntent(reader({ open: 'character-view' }))).toBeNull();
  });

  it('opens the salon list with no payload', () => {
    expect(parseOpenIntent(reader({ open: 'salon-list' }))).toEqual({
      kind: 'salon-list',
      payload: undefined,
    });
  });

  it('carries the drill-in ids for prospero / scriptorium / aurora', () => {
    expect(parseOpenIntent(reader({ open: 'prospero', projectId: 'p1' }))).toEqual({
      kind: 'prospero',
      payload: { projectId: 'p1' },
    });
    expect(parseOpenIntent(reader({ open: 'scriptorium', storeId: 's1' }))).toEqual({
      kind: 'scriptorium',
      payload: { storeId: 's1' },
    });
    expect(parseOpenIntent(reader({ open: 'aurora', groupId: 'g1' }))).toEqual({
      kind: 'aurora',
      payload: { groupId: 'g1' },
    });
    // No id ⇒ the plain list (the tab still opens).
    expect(parseOpenIntent(reader({ open: 'prospero' }))).toEqual({
      kind: 'prospero',
      payload: undefined,
    });
  });

  it('turns a terminal intent into the Salon-parent two-step', () => {
    expect(parseOpenIntent(reader({ open: 'terminal', chatId: 'c1', sessionId: 's9' }))).toEqual({
      kind: 'terminal',
      payload: { chatId: 'c1', sessionId: 's9' },
      parent: { kind: 'salon', payload: { chatId: 'c1' } },
    });
    // No sessionId: still the two-step, with an undefined session.
    expect(parseOpenIntent(reader({ open: 'terminal', chatId: 'c1' }))).toEqual({
      kind: 'terminal',
      payload: { chatId: 'c1', sessionId: undefined },
      parent: { kind: 'salon', payload: { chatId: 'c1' } },
    });
    expect(parseOpenIntent(reader({ open: 'terminal' }))).toBeNull();
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

/**
 * v4's `WorkspaceIntent` component delta, replayed against the real store: v4
 * probes `state.tabs` as `kind:id:tab|sessionId` triples, so this builds the
 * same probe string.
 */
describe('applyOpenIntent — the v4 8d86847a intent-layer delta', () => {
  let svc: WorkspaceService;

  function makeCore(): CoreClient {
    return {
      dispatchExpect: vi.fn(async () => ({ type: 'chats', data: [] })),
    } as unknown as CoreClient;
  }

  /** v4's `Probe`: `${kind}:${id}:${tab ?? sessionId ?? ''}`, sorted. */
  function probe(): string {
    return Object.values(svc.state().tabs)
      .map((t) => {
        const p = (t.payload ?? {}) as {
          characterId?: string;
          tab?: string;
          projectId?: string;
          storeId?: string;
          groupId?: string;
          chatId?: string;
          sessionId?: string;
        };
        const id = p.characterId ?? p.projectId ?? p.storeId ?? p.groupId ?? p.chatId ?? '';
        return `${t.kind}:${id}:${p.tab ?? p.sessionId ?? ''}`;
      })
      .sort()
      .join('|');
  }

  function open(q: Record<string, string>): void {
    const intent = parseOpenIntent(reader(q));
    if (intent) applyOpenIntent(svc, intent);
  }

  beforeEach(() => {
    localStorage.clear();
    svc = new WorkspaceService(makeCore());
  });

  it('opens nothing for a bogus kind', () => {
    open({ open: 'bogus' });
    expect(probe()).toBe('home::');
  });

  it('opens the salon list tab', () => {
    open({ open: 'salon-list' });
    expect(probe()).toContain('salon-list::');
  });

  it('opens the Prospero tab drilled into a project', () => {
    open({ open: 'prospero', projectId: 'p1' });
    expect(probe()).toContain('prospero:p1:');
  });

  it('opens the Scriptorium tab drilled into a store', () => {
    open({ open: 'scriptorium', storeId: 's1' });
    expect(probe()).toContain('scriptorium:s1:');
  });

  it('opens the Aurora tab drilled into a group', () => {
    open({ open: 'aurora', groupId: 'g1' });
    expect(probe()).toContain('aurora:g1:');
  });

  it('opens the character detail view with its characterId + tab payload', () => {
    open({ open: 'character-view', characterId: 'abc', tab: 'conversations' });
    expect(probe()).toContain('character-view:abc:conversations');
  });

  it('skips character-view when the characterId is missing', () => {
    open({ open: 'character-view' });
    expect(probe()).not.toContain('character-view');
  });

  it('opens the conversation plus a child terminal tab for a terminal intent', () => {
    open({ open: 'terminal', chatId: 'c1', sessionId: 's9' });
    const text = probe();
    expect(text).toContain('salon:c1:');
    expect(text).toContain('terminal:c1:s9');
    // The terminal is parented to the Salon tab it portals from, so closing the
    // Salon cascades it (the reducer's child cascade).
    const tabs = Object.values(svc.state().tabs);
    const salon = tabs.find((t) => t.kind === 'salon')!;
    const terminal = tabs.find((t) => t.kind === 'terminal')!;
    expect(terminal.parentTabId).toBe(salon.id);
  });

  // A re-drill re-uses the SAME singleton tab and re-targets its payload (the
  // reducer keeps prospero/scriptorium/aurora keyed by kind alone).
  it('re-drills the open singleton instead of opening a second tab', () => {
    open({ open: 'prospero', projectId: 'p1' });
    open({ open: 'prospero', projectId: 'p2' });
    expect(Object.values(svc.state().tabs).filter((t) => t.kind === 'prospero')).toHaveLength(1);
    expect(probe()).toContain('prospero:p2:');
  });
});
