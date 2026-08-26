import { describe, expect, it, vi } from 'vitest';

import {
  isModifiedClick,
  resolveActiveSalon,
  OpenDocumentFromSearch,
  type ActiveSalon,
} from './open-document-from-search';
import { DOCUMENT_OPENED_EVENT, openDocumentInChat } from './open-document-in-chat';
import type { DocumentApi } from './document-api';
import type { DocumentSearchResultItem } from '../search/search.types';
import type { WorkspaceState, WorkspaceTab } from '../workspace/workspace-contract';

/**
 * Parity specs for the P4.D122 open-from-search choreography, read from v4's
 * `lib/hooks/use-open-document-from-search.ts` +
 * `lib/documents/open-document-in-chat.ts` at `b220999d` and its own 10-case
 * `use-open-document-from-search.test.ts`.
 */

function state(tabs: WorkspaceTab[], activeTabId: string | null, focused: 'left' | 'right' = 'left'): WorkspaceState {
  const byId: Record<string, WorkspaceTab> = {};
  for (const t of tabs) byId[t.id] = t;
  return {
    tabs: byId,
    panes: {
      left: { order: tabs.map((t) => t.id), activeTabId: focused === 'left' ? activeTabId : null },
      right: focused === 'right' ? { order: tabs.map((t) => t.id), activeTabId } : null,
    },
    focusedPane: focused,
    splitRatio: 0.5,
  };
}

const salonTab = (id: string, chatId: unknown): WorkspaceTab => ({
  id,
  kind: 'salon',
  payload: { chatId },
  title: 'Salon',
});

describe('resolveActiveSalon (v4 use-open-document-from-search.ts:57-79)', () => {
  it("picks the focused pane's active Salon tab", () => {
    const s = state([salonTab('t1', 'chat-1')], 't1');
    expect(resolveActiveSalon(s, '/workspace')).toEqual<ActiveSalon>({
      chatId: 'chat-1',
      tabId: 't1',
    });
  });

  it('ignores a Salon idling in the unfocused pane', () => {
    // The Salon is open, but the FOCUSED pane's activeTabId is null.
    const s = state([salonTab('t1', 'chat-1')], null);
    expect(resolveActiveSalon(s, '/workspace')).toBeNull();
  });

  it('ignores a focused non-Salon tab even when a Salon is open elsewhere', () => {
    const s = state(
      [salonTab('t1', 'chat-1'), { id: 't2', kind: 'home', title: 'Home' }],
      't2',
    );
    expect(resolveActiveSalon(s, '/workspace')).toBeNull();
  });

  it('ignores a Salon tab whose payload carries no usable chatId', () => {
    expect(resolveActiveSalon(state([salonTab('t1', '')], 't1'), '/workspace')).toBeNull();
    expect(resolveActiveSalon(state([salonTab('t1', 42)], 't1'), '/workspace')).toBeNull();
    expect(
      resolveActiveSalon(state([{ id: 't1', kind: 'salon', title: 'S' }], 't1'), '/workspace'),
    ).toBeNull();
  });

  it('does NOT fall through to the pathname when workspace state exists', () => {
    // The pathname arm is reachable only with a null state — v4's early return.
    expect(resolveActiveSalon(state([], null), '/salon/chat-9')).toBeNull();
  });

  it('falls back to the /salon/[id] pathname outside the workspace', () => {
    expect(resolveActiveSalon(null, '/salon/chat-9')).toEqual<ActiveSalon>({
      chatId: 'chat-9',
      tabId: null,
    });
    // The id is percent-decoded.
    expect(resolveActiveSalon(null, '/salon/a%20b')?.chatId).toBe('a b');
  });

  it('is null on any other pathname, and on the new-chat form', () => {
    expect(resolveActiveSalon(null, '/salon/new')).toBeNull();
    expect(resolveActiveSalon(null, '/salon/chat-9/extra')).toBeNull();
    expect(resolveActiveSalon(null, '/workspace')).toBeNull();
    expect(resolveActiveSalon(null, null)).toBeNull();
  });
});

describe('isModifiedClick (v4 :82-84)', () => {
  const ev = (over: Partial<MouseEvent>) => ({ button: 0, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...over }) as MouseEvent;

  it('is true for every modifier and for a non-primary button', () => {
    expect(isModifiedClick(ev({ button: 1 }))).toBe(true);
    expect(isModifiedClick(ev({ metaKey: true }))).toBe(true);
    expect(isModifiedClick(ev({ ctrlKey: true }))).toBe(true);
    expect(isModifiedClick(ev({ shiftKey: true }))).toBe(true);
    expect(isModifiedClick(ev({ altKey: true }))).toBe(true);
  });

  it('is false for a plain left click', () => {
    expect(isModifiedClick(ev({}))).toBe(false);
  });
});

// ── The service ──────────────────────────────────────────────────────────────

interface Harness {
  svc: OpenDocumentFromSearch;
  openTab: ReturnType<typeof vi.fn>;
  navigateByUrl: ReturnType<typeof vi.fn>;
  openDocument: ReturnType<typeof vi.fn>;
  showError: ReturnType<typeof vi.fn>;
}

/**
 * The service is constructed by hand rather than through TestBed: it reads the
 * router URL and the workspace signal, both of which are trivially stubbed, and
 * this keeps the parity table readable next to v4's.
 */
function harness(opts: {
  url: string;
  workspaceState?: WorkspaceState;
  workspaceEnabled?: boolean;
  openResult?: unknown;
  openRejects?: Error;
}): Harness {
  const openTab = vi.fn().mockReturnValue('new-tab');
  const navigateByUrl = vi.fn().mockResolvedValue(true);
  const openDocument = opts.openRejects
    ? vi.fn().mockRejectedValue(opts.openRejects)
    : vi.fn().mockResolvedValue(
        opts.openResult ?? { document: { id: 'cd-1', displayTitle: 'Notes' } },
      );
  const showError = vi.fn();

  const svc = Object.create(OpenDocumentFromSearch.prototype) as OpenDocumentFromSearch;
  Object.assign(svc, {
    workspace: { state: () => opts.workspaceState ?? null, openTab },
    router: { url: opts.url, navigateByUrl },
    injector: { get: () => ({ openDocument }) as unknown as DocumentApi },
    toasts: { showError },
  });
  // The flag reader is module-level; stub the private predicate instead so the
  // spec doesn't have to reach into localStorage.
  (svc as unknown as { inWorkspace(): boolean }).inWorkspace = () =>
    (opts.workspaceEnabled ?? true) && opts.url.split('?')[0] === '/workspace';
  return { svc, openTab, navigateByUrl, openDocument, showError };
}

const result: DocumentSearchResultItem = {
  id: 'link-1',
  type: 'documents',
  name: 'manifesto.md',
  matchedField: 'fileName',
  matchedValue: 'manifesto.md',
  snippet: 'Notes/manifesto.md',
  url: '/workspace?open=document-standalone&scope=document_store&mountPoint=Papers&filePath=Notes%2Fmanifesto.md',
  matchPriority: 1,
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
  mountPointId: 'mp-1',
  mountPointName: 'Papers',
  mountPointRef: 'Papers',
  storeType: 'documents',
  relativePath: 'Notes/manifesto.md',
};

function click(over: Partial<MouseEvent> = {}): MouseEvent & { defaultPrevented: boolean } {
  const e = {
    button: 0,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    defaultPrevented: false,
    preventDefault() {
      (this as { defaultPrevented: boolean }).defaultPrevented = true;
    },
    ...over,
  };
  return e as unknown as MouseEvent & { defaultPrevented: boolean };
}

describe('OpenDocumentFromSearch (v4 useOpenDocumentFromSearch:91-146)', () => {
  it('opens in the focused Salon, parented to its tab', async () => {
    const h = harness({ url: '/workspace', workspaceState: state([salonTab('t1', 'chat-1')], 't1') });
    const e = click();
    h.svc.open(result, e);
    expect(e.defaultPrevented).toBe(true);
    await Promise.resolve();
    await Promise.resolve();
    expect(h.openDocument).toHaveBeenCalledWith('chat-1', {
      filePath: 'Notes/manifesto.md',
      scope: 'document_store',
      mountPoint: 'Papers',
      mode: 'split',
    });
    expect(h.openTab).toHaveBeenCalledWith(
      'document',
      { chatId: 'chat-1', chatDocumentId: 'cd-1', displayTitle: 'Notes' },
      { focus: true, parentTabId: 't1' },
    );
    expect(h.navigateByUrl).not.toHaveBeenCalled();
  });

  it('opens a silent standalone tab when the workspace has no Salon focused', () => {
    const h = harness({ url: '/workspace', workspaceState: state([], null) });
    h.svc.open(result, click());
    expect(h.openDocument).not.toHaveBeenCalled();
    expect(h.openTab).toHaveBeenCalledWith(
      'document-standalone',
      {
        docKey: 'document_store:Papers:Notes/manifesto.md',
        scope: 'document_store',
        mountPoint: 'Papers',
        filePath: 'Notes/manifesto.md',
        displayTitle: 'manifesto.md',
      },
      { title: 'manifesto.md' },
    );
  });

  it('opens in the chat the legacy Salon page is showing, without a tab', async () => {
    const h = harness({ url: '/salon/chat-7' });
    h.svc.open(result, click());
    await Promise.resolve();
    await Promise.resolve();
    expect(h.openDocument).toHaveBeenCalledWith(
      'chat-7',
      expect.objectContaining({ mode: 'split' }),
    );
    // Outside the workspace there is no tab to open and no parent to name.
    expect(h.openTab).not.toHaveBeenCalled();
  });

  it('pushes the standalone deep link outside the workspace shell', () => {
    const h = harness({ url: '/characters' });
    h.svc.open(result, click());
    expect(h.openDocument).not.toHaveBeenCalled();
    expect(h.openTab).not.toHaveBeenCalled();
    expect(h.navigateByUrl).toHaveBeenCalledWith(result.url);
  });

  it('leaves modified clicks to the browser', () => {
    const h = harness({ url: '/workspace', workspaceState: state([salonTab('t1', 'chat-1')], 't1') });
    const e = click({ metaKey: true });
    h.svc.open(result, e);
    // No preventDefault → the anchor's standalone href opens in a new tab.
    expect(e.defaultPrevented).toBe(false);
    expect(h.openDocument).not.toHaveBeenCalled();
    expect(h.openTab).not.toHaveBeenCalled();
    expect(h.navigateByUrl).not.toHaveBeenCalled();
  });

  it('toasts the server message when the in-chat open fails', async () => {
    const h = harness({
      url: '/workspace',
      workspaceState: state([salonTab('t1', 'chat-1')], 't1'),
      openRejects: new Error('Document not found'),
    });
    h.svc.open(result, click());
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(h.showError).toHaveBeenCalledWith('Document not found');
  });
});

describe('openDocumentInChat (v4 open-document-in-chat.ts:57-83)', () => {
  it('opens the row, opens the tab, and announces the open', async () => {
    const openDocument = vi
      .fn()
      .mockResolvedValue({ document: { id: 'cd-9', displayTitle: 'Ledger' } });
    const openTab = vi.fn();
    const seen: Array<{ chatId: string; chatDocumentId: string }> = [];
    const listener = (e: Event) =>
      seen.push((e as CustomEvent<{ chatId: string; chatDocumentId: string }>).detail);
    window.addEventListener(DOCUMENT_OPENED_EVENT, listener);

    await openDocumentInChat(
      { openDocument } as unknown as DocumentApi,
      'chat-3',
      { filePath: 'a.md', scope: 'document_store', mountPoint: 'Papers' },
      { openTab, parentTabId: 'tab-3' },
    );

    window.removeEventListener(DOCUMENT_OPENED_EVENT, listener);
    // `mode` defaults to split; a null mountPoint becomes undefined.
    expect(openDocument).toHaveBeenCalledWith('chat-3', {
      filePath: 'a.md',
      scope: 'document_store',
      mountPoint: 'Papers',
      mode: 'split',
    });
    expect(openTab).toHaveBeenCalledWith(
      'document',
      { chatId: 'chat-3', chatDocumentId: 'cd-9', displayTitle: 'Ledger' },
      { focus: true, parentTabId: 'tab-3' },
    );
    expect(seen).toEqual([{ chatId: 'chat-3', chatDocumentId: 'cd-9' }]);
  });

  it('still announces when there is no workspace tab to open', async () => {
    const openDocument = vi
      .fn()
      .mockResolvedValue({ document: { id: 'cd-1', displayTitle: 'X' } });
    const seen: Event[] = [];
    const listener = (e: Event) => seen.push(e);
    window.addEventListener(DOCUMENT_OPENED_EVENT, listener);
    await openDocumentInChat(
      { openDocument } as unknown as DocumentApi,
      'chat-1',
      { filePath: 'a.md', scope: 'general', mountPoint: null, mode: 'focus' },
      { openTab: null },
    );
    window.removeEventListener(DOCUMENT_OPENED_EVENT, listener);
    expect(openDocument).toHaveBeenCalledWith('chat-1', {
      filePath: 'a.md',
      scope: 'general',
      mountPoint: undefined,
      mode: 'focus',
    });
    expect(seen).toHaveLength(1);
  });
});
