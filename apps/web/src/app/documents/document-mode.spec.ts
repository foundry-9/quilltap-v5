import { TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import type { MessageDto } from '../core/core-contract';
import {
  DocumentApi,
  type ActiveDocument,
  type ReadDocumentResult,
  type WriteDocumentOutcome,
} from './document-api';
import {
  computeAbsorbNext,
  dividerPositionFromChat,
  documentModeFromChat,
  DocumentModeController,
  friendlyOpenDocumentError,
  nextFocusedAfterRemove,
  USES_RICH_MARKDOWN_EDITOR,
} from './document-mode';

function record(over: Partial<ActiveDocument> = {}) {
  return {
    id: over.id ?? 'd1',
    filePath: over.filePath ?? 'notes.txt',
    scope: over.scope ?? ('project' as const),
    mountPoint: over.mountPoint,
    displayTitle: over.displayTitle ?? 'Notes',
  };
}

function librarian(id: string): MessageDto {
  return { id, role: 'ASSISTANT', content: `msg ${id}` } as unknown as MessageDto;
}

/** A recording stub of the dispatch client. */
class FakeApi {
  openResult: {
    document: ReturnType<typeof record>;
    content?: string;
    mtime?: number;
    librarianMessage?: MessageDto | null;
  } = { document: record(), content: 'hello', mtime: 1 };
  writeOutcome: WriteDocumentOutcome = { kind: 'saved', mtime: 2, librarianMessage: null };
  readResult: ReadDocumentResult = { content: 'server', mtime: 9 };
  openDocsResult: ReturnType<typeof record>[] = [];
  renameResult: { document: ReturnType<typeof record>; librarianMessage?: MessageDto | null } = {
    document: record({ filePath: 'renamed.txt', displayTitle: 'Renamed' }),
  };
  deleteResult: { librarianMessage: MessageDto | null } = { librarianMessage: null };
  persisted: Array<Record<string, unknown>> = [];
  writtenContent: string[] = [];

  toActiveDocument = (rec: ReturnType<typeof record>, content = '', mtime?: number) =>
    ({
      id: rec.id,
      filePath: rec.filePath,
      scope: rec.scope,
      mountPoint: rec.mountPoint,
      displayTitle: rec.displayTitle || rec.filePath,
      content,
      mtime,
    }) as ActiveDocument;

  openDocument = vi.fn(async () => this.openResult);
  closeDocument = vi.fn(async () => {});
  readDocument = vi.fn(async () => this.readResult);
  writeDocument = vi.fn(async (_c: string, p: { content: string }) => {
    this.writtenContent.push(p.content);
    return this.writeOutcome;
  });
  renameDocument = vi.fn(async () => this.renameResult);
  deleteDocument = vi.fn(async () => this.deleteResult);
  fetchOpenDocuments = vi.fn(async () => this.openDocsResult);
  fetchChatDocumentState = vi.fn(async () => ({ documentMode: 'split' as const }));
  persistChatDocumentState = vi.fn(async (_c: string, u: Record<string, unknown>) => {
    this.persisted.push(u);
  });
}

function build(): { ctrl: DocumentModeController; api: FakeApi; librarianSink: MessageDto[] } {
  const api = new FakeApi();
  TestBed.configureTestingModule({
    providers: [DocumentModeController, { provide: DocumentApi, useValue: api }],
  });
  const ctrl = TestBed.inject(DocumentModeController);
  const librarianSink: MessageDto[] = [];
  ctrl.configure('c1', (m) => librarianSink.push(m));
  return { ctrl, api, librarianSink };
}

describe('document-mode pure helpers (v4)', () => {
  it('documentModeFromChat / dividerPositionFromChat defaults', () => {
    expect(documentModeFromChat(null)).toBe('normal');
    expect(documentModeFromChat({ documentMode: 'focus' })).toBe('focus');
    expect(dividerPositionFromChat(null)).toBe(45);
    expect(dividerPositionFromChat({ dividerPosition: 30 })).toBe(30);
  });

  it('computeAbsorbNext follows the shipped editor (textarea by default)', () => {
    // With the textarea editor (spike RED default) markdown never re-serializes.
    expect(computeAbsorbNext('a.md', 'body', true)).toBe(USES_RICH_MARKDOWN_EDITOR);
    // A plain-text file never absorbs regardless.
    expect(computeAbsorbNext('a.txt', 'body', true)).toBe(false);
    // No version bump → never absorb.
    expect(computeAbsorbNext('a.md', 'body', false)).toBe(false);
  });

  it('nextFocusedAfterRemove keeps a non-focused pointer, else picks a neighbor', () => {
    const order = [
      { document: record({ id: 'a' }) },
      { document: record({ id: 'b' }) },
      { document: record({ id: 'c' }) },
    ] as never;
    expect(nextFocusedAfterRemove(order, 'a', 'b')).toBe('b');
    expect(nextFocusedAfterRemove(order, 'b', 'b')).toBe('c');
    expect(nextFocusedAfterRemove([{ document: record({ id: 'a' }) }] as never, 'a', 'a')).toBeNull();
  });

  it('friendlyOpenDocumentError rewrites File-not-found', () => {
    expect(friendlyOpenDocumentError('File not found: x', 'x.md', undefined)).toContain(
      'file not found',
    );
    expect(friendlyOpenDocumentError('boom', undefined, undefined)).toBe('boom');
  });
});

describe('DocumentModeController (v4 useDocumentMode)', () => {
  it('openDocument loads content, focuses, enters split, appends the Librarian message', async () => {
    const { ctrl, api, librarianSink } = build();
    api.openResult = { document: record({ id: 'd1' }), content: 'hello', mtime: 1, librarianMessage: librarian('L1') };

    const doc = await ctrl.openDocument({ filePath: 'notes.txt', scope: 'project' });

    expect(doc?.id).toBe('d1');
    expect(ctrl.documentActive()).toBe(true);
    expect(ctrl.documentMode()).toBe('split');
    expect(ctrl.focusedDocId()).toBe('d1');
    expect(ctrl.activeDocument()?.content).toBe('hello');
    expect(librarianSink.map((m) => m.id)).toEqual(['L1']);
  });

  it('handleContentChange flags dirty; identical content clears it', async () => {
    const { ctrl } = build();
    await ctrl.openDocument({ filePath: 'notes.txt' });
    ctrl.handleContentChange('d1', 'hello!!');
    expect(ctrl.openDocs()[0].isDirty).toBe(true);
    ctrl.handleContentChange('d1', 'hello'); // back to the saved baseline
    expect(ctrl.openDocs()[0].isDirty).toBe(false);
  });

  it('saveDocument writes the diff, clears dirty, appends a save announcement', async () => {
    const { ctrl, api, librarianSink } = build();
    api.writeOutcome = { kind: 'saved', mtime: 5, librarianMessage: librarian('S1') };
    await ctrl.openDocument({ filePath: 'notes.txt' });
    ctrl.handleContentChange('d1', 'hello world');
    await ctrl.saveDocument('d1');

    expect(api.writtenContent.at(-1)).toBe('hello world');
    expect(ctrl.openDocs()[0].isDirty).toBe(false);
    expect(ctrl.openDocs()[0].document.mtime).toBe(5);
    expect(librarianSink.map((m) => m.id)).toEqual(['S1']);
  });

  it('a 409 conflict re-reads the server (no retry of the failed write)', async () => {
    const { ctrl, api } = build();
    await ctrl.openDocument({ filePath: 'notes.txt' });
    ctrl.handleContentChange('d1', 'edited');
    api.writeOutcome = { kind: 'conflict' };
    api.readResult = { content: 'server version', mtime: 42 };
    await ctrl.saveDocument('d1');

    expect(api.readDocument).toHaveBeenCalled();
    // The write is NOT retried — exactly one write attempt was made.
    expect(api.writeDocument).toHaveBeenCalledTimes(1);
  });

  it('a 409 conflict WITH local edits keeps the edit and only refreshes mtime', async () => {
    const { ctrl, api } = build();
    await ctrl.openDocument({ filePath: 'notes.txt' });
    ctrl.handleContentChange('d1', 'my unsaved edit');
    api.writeOutcome = { kind: 'conflict' };
    api.readResult = { content: 'someone elses version', mtime: 77 };
    await ctrl.saveDocument('d1');

    expect(ctrl.activeDocument()?.content).toBe('my unsaved edit');
    expect(ctrl.activeDocument()?.mtime).toBe(77);
  });

  it('closeDocument returns to normal and persists once the last doc closes', async () => {
    const { ctrl, api } = build();
    await ctrl.openDocument({ filePath: 'notes.txt' });
    await ctrl.closeDocument('d1');

    expect(api.closeDocument).toHaveBeenCalledWith('c1', 'd1');
    expect(ctrl.documentActive()).toBe(false);
    expect(ctrl.documentMode()).toBe('normal');
    expect(api.persisted.at(-1)).toEqual({ documentMode: 'normal' });
  });

  it('renameDocument rewrites the path/title and appends the Librarian message', async () => {
    const { ctrl, api, librarianSink } = build();
    api.renameResult = {
      document: record({ id: 'd1', filePath: 'renamed.txt', displayTitle: 'Renamed' }),
      librarianMessage: librarian('R1'),
    };
    await ctrl.openDocument({ filePath: 'notes.txt' });
    await ctrl.renameDocument('d1', 'Renamed');

    expect(ctrl.activeDocument()?.displayTitle).toBe('Renamed');
    expect(ctrl.activeDocument()?.filePath).toBe('renamed.txt');
    expect(librarianSink.map((m) => m.id)).toEqual(['R1']);
  });

  it('deleteDocument removes the pane and appends the Librarian message', async () => {
    const { ctrl, api, librarianSink } = build();
    api.deleteResult = { librarianMessage: librarian('D1') };
    await ctrl.openDocument({ filePath: 'notes.txt' });
    await ctrl.deleteDocument('d1');

    expect(ctrl.documentActive()).toBe(false);
    expect(ctrl.documentMode()).toBe('normal');
    expect(librarianSink.map((m) => m.id)).toEqual(['D1']);
  });

  it('hydrate(normal) clears open docs; hydrate(split) reconciles', async () => {
    const { ctrl, api } = build();
    await ctrl.openDocument({ filePath: 'notes.txt' });
    ctrl.hydrate({ documentMode: 'normal', dividerPosition: 45 });
    expect(ctrl.documentActive()).toBe(false);

    api.openDocsResult = [record({ id: 'd2', filePath: 'from-server.txt' })];
    ctrl.hydrate({ documentMode: 'split', dividerPosition: 60 });
    await flush();
    expect(ctrl.dividerPosition()).toBe(60);
    expect(ctrl.openDocs().map((e) => e.document.id)).toEqual(['d2']);
  });

  it('handleLLMEditEnd re-reads a clean pane but skips a dirty one', async () => {
    const { ctrl, api } = build();
    await ctrl.openDocument({ filePath: 'notes.txt' });
    api.readResult = { content: 'edited by the LLM', mtime: 100 };
    await ctrl.handleLLMEditEnd();
    expect(ctrl.activeDocument()?.content).toBe('edited by the LLM');

    // Now make it dirty and confirm a subsequent re-read is skipped.
    ctrl.handleContentChange('d1', 'my local edit');
    api.readResult = { content: 'another LLM write', mtime: 200 };
    await ctrl.handleLLMEditEnd();
    expect(ctrl.activeDocument()?.content).toBe('my local edit');
  });

  it('reloadFromServer reconciles the open set to what the server reports', async () => {
    const { ctrl, api } = build();
    await ctrl.openDocument({ filePath: 'notes.txt' });
    // Server now reports a different doc open; d1 should drop, d9 should appear.
    api.fetchChatDocumentState = vi.fn(async () => ({ documentMode: 'split' as const }));
    api.openDocsResult = [record({ id: 'd9', filePath: 'server.txt' })];
    api.readResult = { content: 'server body', mtime: 1 };
    await ctrl.reloadFromServer();
    expect(ctrl.openDocs().map((e) => e.document.id)).toEqual(['d9']);
    expect(ctrl.activeDocument()?.content).toBe('server body');
  });

  it('setDividerPosition + toggleFocusMode persist to the chat record', async () => {
    const { ctrl, api } = build();
    ctrl.setDividerPosition(30);
    expect(ctrl.dividerPosition()).toBe(30);
    ctrl.toggleFocusMode();
    expect(ctrl.documentMode()).toBe('focus');
    await flush();
    expect(api.persisted).toContainEqual({ dividerPosition: 30 });
    expect(api.persisted).toContainEqual({ documentMode: 'focus' });
  });
});

/** Drain the async open/reconcile microtasks. */
async function flush(): Promise<void> {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}
