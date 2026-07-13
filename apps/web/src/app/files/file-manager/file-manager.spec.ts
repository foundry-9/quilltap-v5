import { ComponentFixture, TestBed } from '@angular/core/testing';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { FileManager } from './file-manager';
import type { MountCapabilities } from '../types';

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

interface Row {
  relativePath: string;
  fileSizeBytes?: number | null;
  lastModified?: string | number | null;
}

/** A minimal CoreClient stand-in: canned mountFilesList + recorded verbs. */
class FakeCore {
  listing: { files: Row[]; folders: string[] } = { files: [], folders: [] };
  requests: { type: string; [k: string]: unknown }[] = [];
  errorFor: Record<string, { kind?: string; code?: string; message?: string }> = {};

  dispatch = vi.fn((req: { type: string; [k: string]: unknown }) => {
    this.requests.push(req);
    if (req.type === 'mountFilesList') {
      return Promise.resolve({ type: 'mountFile', data: this.listing });
    }
    const err = this.errorFor[req.type];
    if (err) return Promise.resolve({ type: 'error', data: err });
    return Promise.resolve({ type: 'mountFile', data: {} });
  });

  verbs(): string[] {
    return this.requests.map((r) => r.type);
  }
  mutations(): { type: string; [k: string]: unknown }[] {
    return this.requests.filter((r) => r.type !== 'mountFilesList');
  }
}

/** Drain the async dispatch/reload microtasks (zoneless whenStable doesn't). */
async function flush(): Promise<void> {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}

async function mount(caps: MountCapabilities, listing: { files: Row[]; folders: string[] }) {
  const core = new FakeCore();
  core.listing = listing;
  TestBed.configureTestingModule({
    imports: [FileManager],
    providers: [{ provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(FileManager);
  fixture.componentRef.setInput('mountPointId', 'm1');
  fixture.componentRef.setInput('capabilities', caps);
  fixture.componentRef.setInput('mountType', 'database');
  fixture.detectChanges();
  await flush();
  fixture.detectChanges();
  return { fixture, core };
}

function html(fixture: ComponentFixture<FileManager>): string {
  return (fixture.nativeElement as HTMLElement).innerHTML;
}
function byTestId(fixture: ComponentFixture<FileManager>, id: string): HTMLElement | null {
  return (fixture.nativeElement as HTMLElement).querySelector(`[data-testid="${id}"]`);
}
function allByTestId(fixture: ComponentFixture<FileManager>, id: string): HTMLElement[] {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll(`[data-testid="${id}"]`));
}

async function settle(fixture: ComponentFixture<FileManager>) {
  await flush();
  fixture.detectChanges();
  await flush();
  fixture.detectChanges();
}

describe('FileManager — rendering + gating', () => {
  beforeEach(() => vi.stubGlobal('open', vi.fn()));

  it('shows the loading line, then the listing (folders + files)', async () => {
    const { fixture } = await mount(ALL, {
      files: [{ relativePath: 'Report.md', fileSizeBytes: 10, lastModified: null }],
      folders: ['Drafts'],
    });
    expect(byTestId(fixture, 'fm-loading')).toBeNull();
    expect(byTestId(fixture, 'fm-folder-row')?.textContent).toContain('Drafts');
    expect(byTestId(fixture, 'fm-file-row')?.textContent).toContain('Report.md');
  });

  it('empty listing shows the steampunk empty line', async () => {
    const { fixture } = await mount(ALL, { files: [], folders: [] });
    expect(byTestId(fixture, 'fm-empty')?.textContent).toContain('This drawer stands empty');
  });

  it('a read-only mount hides every mutating affordance', async () => {
    const { fixture } = await mount(READONLY, {
      files: [{ relativePath: 'a.txt', fileSizeBytes: 1, lastModified: null }],
      folders: [],
    });
    expect(byTestId(fixture, 'fm-new-folder')).toBeNull();
    expect(byTestId(fixture, 'fm-upload')).toBeNull();
    expect(byTestId(fixture, 'fm-rename')).toBeNull();
    expect(byTestId(fixture, 'fm-delete')).toBeNull();
    expect(byTestId(fixture, 'fm-cut')).toBeNull();
    // Download stays available (a read source).
    expect(byTestId(fixture, 'fm-download')).not.toBeNull();
  });
});

describe('FileManager — mutations', () => {
  beforeEach(() => vi.stubGlobal('open', vi.fn()));

  it('create folder dispatches mountFolderCreate then reloads', async () => {
    const { fixture, core } = await mount(ALL, { files: [], folders: [] });
    byTestId(fixture, 'fm-new-folder')!.click();
    fixture.detectChanges();
    const input = byTestId(fixture, 'fm-new-folder-input') as HTMLInputElement;
    input.value = 'Sub';
    byTestId(fixture, 'fm-new-folder-confirm')!.click();
    await settle(fixture);
    const create = core.mutations().find((r) => r.type === 'mountFolderCreate');
    expect(create).toEqual({ type: 'mountFolderCreate', mountPointId: 'm1', path: 'Sub' });
    // A reload followed the mutation.
    expect(core.verbs().filter((v) => v === 'mountFilesList').length).toBeGreaterThanOrEqual(2);
  });

  it('inline rename dispatches mountFileUpdate with the new path', async () => {
    const { fixture, core } = await mount(ALL, {
      files: [{ relativePath: 'old.txt', fileSizeBytes: 1, lastModified: null }],
      folders: [],
    });
    byTestId(fixture, 'fm-rename')!.click();
    fixture.detectChanges();
    const input = byTestId(fixture, 'fm-rename-input') as HTMLInputElement;
    input.value = 'new.txt';
    byTestId(fixture, 'fm-rename-confirm')!.click();
    await settle(fixture);
    expect(core.mutations()).toContainEqual({
      type: 'mountFileUpdate',
      mountPointId: 'm1',
      path: 'old.txt',
      rename: 'new.txt',
    });
  });

  it('delete dispatches mountFileDelete', async () => {
    const { fixture, core } = await mount(ALL, {
      files: [{ relativePath: 'a.txt', fileSizeBytes: 1, lastModified: null }],
      folders: [],
    });
    byTestId(fixture, 'fm-delete')!.click();
    await settle(fixture);
    expect(core.mutations()).toContainEqual({
      type: 'mountFileDelete',
      mountPointId: 'm1',
      path: 'a.txt',
    });
  });

  it('navigates into a folder and lists its children', async () => {
    const { fixture } = await mount(ALL, {
      files: [{ relativePath: 'Drafts/idea.md', fileSizeBytes: 2, lastModified: null }],
      folders: ['Drafts'],
    });
    // Root shows only the Drafts folder, not the nested file.
    expect(html(fixture)).toContain('Drafts');
    expect(byTestId(fixture, 'fm-file-row')).toBeNull();
    // Descend.
    byTestId(fixture, 'fm-folder-row')!.querySelector('button')!.click();
    fixture.detectChanges();
    expect(byTestId(fixture, 'fm-file-row')?.textContent).toContain('idea.md');
    expect(byTestId(fixture, 'fm-breadcrumbs')?.textContent).toContain('Drafts');
  });
});

describe('FileManager — move/copy clipboard + refusals', () => {
  beforeEach(() => vi.stubGlobal('open', vi.fn()));

  it('cut a file → paste in a folder dispatches mountFileMove', async () => {
    const { fixture, core } = await mount(ALL, {
      files: [{ relativePath: 'note.md', fileSizeBytes: 1, lastModified: null }],
      folders: ['Dest'],
    });
    // Cut the file (move mode) — target the file row's cut button.
    (byTestId(fixture, 'fm-file-row')!.querySelector('[data-testid="fm-cut"]') as HTMLElement).click();
    fixture.detectChanges();
    // Navigate into Dest.
    byTestId(fixture, 'fm-folder-row')!.querySelector('button')!.click();
    fixture.detectChanges();
    // Paste here.
    byTestId(fixture, 'fm-paste')!.click();
    await settle(fixture);
    expect(core.mutations()).toContainEqual({
      type: 'mountFileMove',
      mountPointId: 'm1',
      sourcePath: 'note.md',
      destMountPointId: 'm1',
      destPath: 'Dest/note.md',
    });
  });

  it('copy a FOLDER → paste surfaces the refusal and fires NO request', async () => {
    const { fixture, core } = await mount(ALL, {
      files: [],
      folders: ['Src', 'Dest'],
    });
    // Copy the Src folder.
    const srcRow = allByTestId(fixture, 'fm-folder-row').find((r) => r.textContent?.includes('Src'))!;
    (srcRow.querySelector('[data-testid="fm-copy"]') as HTMLElement).click();
    fixture.detectChanges();
    // Navigate into Dest.
    const destRow = allByTestId(fixture, 'fm-folder-row').find((r) => r.textContent?.includes('Dest'))!;
    destRow.querySelector('button')!.click();
    fixture.detectChanges();
    // Paste → refusal.
    byTestId(fixture, 'fm-paste')!.click();
    await settle(fixture);
    expect(byTestId(fixture, 'fm-error')?.textContent).toContain('select the files within');
    expect(core.mutations()).toHaveLength(0);
  });
});

describe('FileManager — reads', () => {
  it('open navigates the browser to the raw item route', async () => {
    const openSpy = vi.fn();
    vi.stubGlobal('open', openSpy);
    const { fixture } = await mount(ALL, {
      files: [{ relativePath: 'Report.md', fileSizeBytes: 1, lastModified: null }],
      folders: [],
    });
    byTestId(fixture, 'fm-file-row')!.querySelector('button')!.click();
    expect(openSpy).toHaveBeenCalledWith(
      '/api/v1/mount-points/m1/files/Report.md',
      '_blank',
      'noopener',
    );
  });
});
