// Cases derived from v4 `components/files/svar/createSvarAdapter.ts`
// (choreography: serialized chain, capability gate + revert, reload-to-
// reconcile, error translation, the unsupported refusal, post-copy reindex).
import { describe, expect, it, vi } from 'vitest';

import {
  FileManagerAdapter,
  type AdapterCallbacks,
  type FileManagerTransport,
  type MountVerbRequest,
  type WireResult,
} from './file-manager-adapter';
import type { ResolvedNode } from './event-wire-map';
import type { MountCapabilities } from './types';

const ALL: MountCapabilities = {
  canWrite: true,
  canDelete: true,
  canCreateFolder: true,
  canMoveIn: true,
  canMoveOut: true,
  canConvert: true,
};
const READONLY: MountCapabilities = {
  canWrite: false,
  canDelete: false,
  canCreateFolder: false,
  canMoveIn: false,
  canMoveOut: false,
  canConvert: false,
};

const file = (relativePath: string, mountId = 'm1'): ResolvedNode => ({
  mountId,
  relativePath,
  type: 'file',
});
const folder = (relativePath: string, mountId = 'm1'): ResolvedNode => ({
  mountId,
  relativePath,
  type: 'folder',
});

class FakeTransport implements FileManagerTransport {
  dispatched: MountVerbRequest[] = [];
  uploads: { mountId: string; path: string; file: File }[] = [];
  /** Per-verb canned result; default ok. */
  results: Record<string, WireResult> = {};

  dispatch = vi.fn((request: MountVerbRequest): Promise<WireResult> => {
    this.dispatched.push(request);
    return Promise.resolve(this.results[request.type] ?? { ok: true, data: {} });
  });
  writeFile = vi.fn((mountId: string, path: string, f: File): Promise<WireResult> => {
    this.uploads.push({ mountId, path, file: f });
    return Promise.resolve(this.results['__upload__'] ?? { ok: true, data: {} });
  });
}

function make(caps: MountCapabilities, cb: AdapterCallbacks = {}) {
  const transport = new FakeTransport();
  const adapter = new FileManagerAdapter(transport, { mountId: 'm1', capabilities: caps }, cb);
  return { transport, adapter };
}

describe('FileManagerAdapter — happy paths', () => {
  it('rename dispatches mountFileUpdate then reloads', async () => {
    const onReloadNeeded = vi.fn();
    const { transport, adapter } = make(ALL, { onReloadNeeded });
    await adapter.rename(file('a/old.txt'), 'new.txt');
    expect(transport.dispatched).toEqual([
      { type: 'mountFileUpdate', mountPointId: 'm1', path: 'a/old.txt', rename: 'a/new.txt' },
    ]);
    expect(onReloadNeeded).toHaveBeenCalledTimes(1);
  });

  it('createFolder dispatches mountFolderCreate', async () => {
    const { transport, adapter } = make(ALL);
    await adapter.createFolder(folder('Docs'), 'Sub');
    expect(transport.dispatched[0]).toEqual({
      type: 'mountFolderCreate',
      mountPointId: 'm1',
      path: 'Docs/Sub',
    });
  });

  it('uploadFile posts the multipart write-file leg with the File', async () => {
    const { transport, adapter } = make(ALL);
    const f = new File(['hi'], 'note.md');
    await adapter.uploadFile(folder('Docs'), f);
    expect(transport.uploads).toEqual([{ mountId: 'm1', path: 'Docs/note.md', file: f }]);
    expect(transport.dispatched).toHaveLength(0);
  });

  it('delete dispatches the right verb per node kind', async () => {
    const { transport, adapter } = make(ALL);
    await adapter.delete([file('a.txt'), folder('Dir')]);
    expect(transport.dispatched.map((r) => r.type)).toEqual([
      'mountFileDelete',
      'mountFolderDelete',
    ]);
  });

  it('openUrl / downloadUrl build the raw item routes (no network)', () => {
    const { transport, adapter } = make(ALL);
    expect(adapter.openUrl(file('x.md'))).toBe('/api/v1/mount-points/m1/files/x.md');
    expect(adapter.downloadUrl(file('x.md'))).toBe('/api/v1/mount-points/m1/files/x.md?raw=1');
    expect(adapter.openUrl(folder('Dir'))).toBeNull();
    expect(transport.dispatch).not.toHaveBeenCalled();
  });
});

describe('FileManagerAdapter — capability gate', () => {
  it('a read-only mount refuses a mutation, fires onError + reverts (no request)', async () => {
    const onError = vi.fn();
    const onReloadNeeded = vi.fn();
    const { transport, adapter } = make(READONLY, { onError, onReloadNeeded });
    await adapter.rename(file('a.txt'), 'b.txt');
    expect(transport.dispatch).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith('This repository won’t accept changes just now.', {
      suggestCopy: false,
      conflict: false,
    });
    expect(onReloadNeeded).toHaveBeenCalledTimes(1); // revert the optimistic change
  });

  it('createFolder gates on canCreateFolder specifically', async () => {
    const caps = { ...READONLY, canCreateFolder: true };
    const { transport, adapter } = make(caps);
    await adapter.createFolder(null, 'Top');
    expect(transport.dispatched[0].type).toBe('mountFolderCreate');
  });
});

describe('FileManagerAdapter — refusal arms (backend gaps)', () => {
  it('a whole-folder copy surfaces the refusal and fires NO request', async () => {
    const onError = vi.fn();
    const { transport, adapter } = make(ALL, { onError });
    await adapter.copy([folder('src')], folder('dst'));
    expect(transport.dispatch).not.toHaveBeenCalled();
    expect(onError.mock.calls[0][0]).toContain('select the files within');
  });

  it('a cross-mount folder move surfaces the copy-across refusal, no request', async () => {
    const onError = vi.fn();
    const { transport, adapter } = make(ALL, { onError });
    await adapter.move([folder('src', 'm1')], folder('dst', 'm2'));
    expect(transport.dispatch).not.toHaveBeenCalled();
    expect(onError.mock.calls[0][0]).toContain('copy the files across');
  });
});

describe('FileManagerAdapter — error translation', () => {
  it('a coded DEST_EXISTS failure surfaces the conflict verdict', async () => {
    const onError = vi.fn();
    const { transport, adapter } = make(ALL, { onError });
    transport.results['mountFileMove'] = { ok: false, kind: 'conflict', code: 'DEST_EXISTS' };
    await adapter.move([file('a.txt')], folder('dst'));
    expect(onError).toHaveBeenCalledTimes(1);
    const [message, info] = onError.mock.calls[0];
    expect(message).toContain('already keeps that desk');
    expect(info).toEqual({ suggestCopy: false, conflict: true });
  });

  it('an UNSUPPORTED code offers copy-instead', async () => {
    const onError = vi.fn();
    const { transport, adapter } = make(ALL, { onError });
    transport.results['mountFileMove'] = { ok: false, code: 'UNSUPPORTED' };
    await adapter.move([file('a.txt')], folder('dst', 'm2'));
    expect(onError.mock.calls[0][1]).toEqual({ suggestCopy: true, conflict: false });
  });
});

describe('FileManagerAdapter — post-copy reindex', () => {
  it('a byte-copy of a .pdf fires onIndexing + mountReindex + mountEmbed', async () => {
    const onIndexing = vi.fn();
    const { transport, adapter } = make(ALL, { onIndexing });
    transport.results['mountFileCopy'] = {
      ok: true,
      data: { strategy: 'byte-copy', destMountPointId: 'db1', destPath: 'in/Doc.pdf' },
    };
    await adapter.copy([file('Doc.pdf')], folder('in', 'db1'));
    expect(onIndexing).toHaveBeenCalledWith({ mountId: 'db1', path: 'in/Doc.pdf' });
    // Give the fire-and-forget reindex a tick to run.
    await Promise.resolve();
    await Promise.resolve();
    const verbs = transport.dispatched.map((r) => r.type);
    expect(verbs).toContain('mountReindex');
    expect(verbs).toContain('mountEmbed');
  });

  it('a non-extractable byte-copy does NOT reindex', async () => {
    const onIndexing = vi.fn();
    const { transport, adapter } = make(ALL, { onIndexing });
    transport.results['mountFileCopy'] = {
      ok: true,
      data: { strategy: 'byte-copy', destMountPointId: 'db1', destPath: 'in/pic.png' },
    };
    await adapter.copy([file('pic.png')], folder('in', 'db1'));
    await Promise.resolve();
    expect(onIndexing).not.toHaveBeenCalled();
    expect(transport.dispatched.map((r) => r.type)).not.toContain('mountReindex');
  });
});

describe('FileManagerAdapter — serialized mutation chain', () => {
  it('runs mutations strictly in order even when overlapped', async () => {
    const order: string[] = [];
    const transport = new FakeTransport();
    transport.dispatch = vi.fn((request: MountVerbRequest) => {
      order.push(`start:${request.type}`);
      return new Promise<WireResult>((resolve) =>
        setTimeout(() => {
          order.push(`end:${request.type}`);
          resolve({ ok: true, data: {} });
        }, 0),
      );
    });
    const adapter = new FileManagerAdapter(transport, { mountId: 'm1', capabilities: ALL });
    // Fire two mutations without awaiting the first.
    const p1 = adapter.delete([file('a.txt')]);
    const p2 = adapter.delete([file('b.txt')]);
    await Promise.all([p1, p2]);
    // The second op must not start until the first ends.
    expect(order).toEqual([
      'start:mountFileDelete',
      'end:mountFileDelete',
      'start:mountFileDelete',
      'end:mountFileDelete',
    ]);
  });
});
