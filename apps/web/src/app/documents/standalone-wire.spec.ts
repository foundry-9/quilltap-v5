import { TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreRequest, CoreResponse } from '../core/core-contract';
import { StandaloneDocumentApi } from './standalone-wire';

/**
 * A CoreClient stub that records the dispatched requests and replays a queue of
 * canned responses. `dispatchData` mirrors the real client: it unwraps `data`
 * and throws on an `error` envelope.
 */
class FakeCore {
  readonly requests: CoreRequest[] = [];
  private readonly responses: CoreResponse[] = [];

  queue(resp: CoreResponse): void {
    this.responses.push(resp);
  }

  dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
    this.requests.push(req);
    return this.responses.shift() ?? ({ type: 'ok', data: {} } as unknown as CoreResponse);
  });

  async dispatchData(req: CoreRequest): Promise<Record<string, unknown>> {
    const resp = await this.dispatch(req);
    if (resp.type === 'error') throw new Error('dispatch error');
    return (resp.data ?? {}) as Record<string, unknown>;
  }
}

function ok(data: unknown): CoreResponse {
  return { type: 'ok', data } as unknown as CoreResponse;
}

function make(): { api: StandaloneDocumentApi; core: FakeCore } {
  const core = new FakeCore();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [StandaloneDocumentApi, { provide: CoreClient, useValue: core }],
  });
  return { api: TestBed.inject(StandaloneDocumentApi), core };
}

describe('StandaloneDocumentApi wire shapes', () => {
  it('fetchStores dispatches documentStores and returns the stores array', async () => {
    const { api, core } = make();
    core.queue(ok({ stores: [{ mountPointId: 'm1' }], projectLibrary: null }));
    const stores = await api.fetchStores();
    expect(core.requests[0]).toEqual({ type: 'documentStores' });
    expect(stores).toEqual([{ mountPointId: 'm1' }]);
  });

  it('fetchRecent dispatches documentsRecent and returns the documents array', async () => {
    const { api, core } = make();
    core.queue(ok({ documents: [{ id: 'r1', filePath: 'a.md', scope: 'general' }] }));
    const recents = await api.fetchRecent();
    expect(core.requests[0]).toEqual({ type: 'documentsRecent' });
    expect(recents).toEqual([{ id: 'r1', filePath: 'a.md', scope: 'general' }]);
  });

  it('open sends the flattened body and reads document/content/mtime/isNew', async () => {
    const { api, core } = make();
    core.queue(
      ok({
        document: { filePath: 'notes/x.md', scope: 'document_store', mountPoint: 'Lib', displayTitle: 'X' },
        content: '# X',
        mtime: 42,
        isNew: false,
      }),
    );
    const result = await api.open({
      filePath: 'notes/x.md',
      title: 'X',
      scope: 'document_store',
      mountPoint: 'Lib',
      targetFolder: 'notes',
    });
    expect(core.requests[0]).toEqual({
      type: 'documentOpen',
      filePath: 'notes/x.md',
      title: 'X',
      scope: 'document_store',
      mountPoint: 'Lib',
      targetFolder: 'notes',
    });
    expect(result).toEqual({
      document: { filePath: 'notes/x.md', scope: 'document_store', mountPoint: 'Lib', displayTitle: 'X' },
      content: '# X',
      mtime: 42,
      isNew: false,
    });
  });

  it('open maps a null mountPoint to undefined (a blank general document)', async () => {
    const { api, core } = make();
    core.queue(ok({ document: { filePath: 'Untitled Document.md', scope: 'general', displayTitle: 'Untitled Document' } }));
    await api.open({ scope: 'general', mountPoint: null });
    expect(core.requests[0]).toEqual({
      type: 'documentOpen',
      filePath: undefined,
      title: undefined,
      scope: 'general',
      mountPoint: undefined,
      targetFolder: undefined,
    });
  });

  it('read sends filePath/scope/mountPoint and returns content + mtime', async () => {
    const { api, core } = make();
    core.queue(ok({ content: 'body', mtime: 7 }));
    const read = await api.read({ filePath: 'a.md', scope: 'general' });
    expect(core.requests[0]).toEqual({
      type: 'documentRead',
      filePath: 'a.md',
      scope: 'general',
      mountPoint: undefined,
    });
    expect(read).toEqual({ content: 'body', mtime: 7 });
  });

  it('write returns a saved outcome carrying the new mtime', async () => {
    const { api, core } = make();
    core.queue(ok({ success: true, mtime: 99 }));
    const outcome = await api.write({ filePath: 'a.md', scope: 'general', content: 'new', mtime: 5 });
    expect(core.requests[0]).toEqual({
      type: 'documentWrite',
      filePath: 'a.md',
      scope: 'general',
      mountPoint: undefined,
      content: 'new',
      mtime: 5,
    });
    expect(outcome).toEqual({ kind: 'saved', mtime: 99 });
  });

  it('write maps a conflict error envelope to the conflict outcome (not retried)', async () => {
    const { api, core } = make();
    core.queue({ type: 'error', data: { kind: 'conflict', message: 'Document changed elsewhere.' } } as unknown as CoreResponse);
    const outcome = await api.write({ filePath: 'a.md', scope: 'general', content: 'new', mtime: 5 });
    expect(outcome).toEqual({ kind: 'conflict' });
  });

  it('write maps a non-conflict error envelope to the error outcome', async () => {
    const { api, core } = make();
    core.queue({ type: 'error', data: { kind: 'internal', message: 'boom' } } as unknown as CoreResponse);
    const outcome = await api.write({ filePath: 'a.md', scope: 'general', content: 'new' });
    expect(outcome).toEqual({ kind: 'error', message: 'boom' });
  });

  it('rename sends newTitle and returns the relocated record', async () => {
    const { api, core } = make();
    core.queue(ok({ document: { filePath: 'Renamed.md', scope: 'general', displayTitle: 'Renamed' } }));
    const result = await api.rename({ filePath: 'a.md', scope: 'general', newTitle: 'Renamed' });
    expect(core.requests[0]).toEqual({
      type: 'documentRename',
      filePath: 'a.md',
      scope: 'general',
      mountPoint: undefined,
      newTitle: 'Renamed',
    });
    expect(result).toEqual({ document: { filePath: 'Renamed.md', scope: 'general', displayTitle: 'Renamed' } });
  });

  it('remove dispatches documentDelete', async () => {
    const { api, core } = make();
    core.queue(ok({ success: true }));
    await api.remove({ filePath: 'a.md', scope: 'document_store', mountPoint: 'Lib' });
    expect(core.requests[0]).toEqual({
      type: 'documentDelete',
      filePath: 'a.md',
      scope: 'document_store',
      mountPoint: 'Lib',
    });
  });

  it('listMountFiles rides the shared mountFilesList verb', async () => {
    const { api, core } = make();
    core.queue(ok({ files: [{ id: 'f1', relativePath: 'top.md' }], folders: ['sub'] }));
    const result = await api.listMountFiles('m1');
    expect(core.requests[0]).toEqual({ type: 'mountFilesList', mountPointId: 'm1' });
    expect(result).toEqual({ files: [{ id: 'f1', relativePath: 'top.md' }], folders: ['sub'] });
  });
});
