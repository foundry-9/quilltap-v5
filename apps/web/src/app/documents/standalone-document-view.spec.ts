import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  WORKSPACE_HANDLE,
  WORKSPACE_TAB_ID,
  type DocumentStandaloneTabPayload,
} from '../workspace/workspace-contract';
import { StandaloneDocumentView } from './standalone-document-view';
import {
  StandaloneDocumentApi,
  type StandaloneOpenResult,
  type StandaloneReadResult,
  type StandaloneRenameResult,
  type StandaloneWriteOutcome,
} from './standalone-wire';

/** A canned {@link StandaloneDocumentApi} recording calls + replaying results. */
class FakeApi {
  openResult: StandaloneOpenResult = {
    document: { filePath: 'notes.md', scope: 'document_store', mountPoint: 'Lib', displayTitle: 'Notes' },
    content: '# Notes\n',
    mtime: 100,
    isNew: false,
  };
  readResult: StandaloneReadResult = { content: '# Disk\n', mtime: 200 };
  writeOutcome: StandaloneWriteOutcome = { kind: 'saved', mtime: 101 };
  renameResult: StandaloneRenameResult = {
    document: { filePath: 'Renamed.md', scope: 'document_store', mountPoint: 'Lib', displayTitle: 'Renamed' },
  };

  open = vi.fn(async () => this.openResult);
  read = vi.fn(async () => this.readResult);
  write = vi.fn(async () => this.writeOutcome);
  rename = vi.fn(async () => this.renameResult);
  remove = vi.fn(async () => undefined);
  listMountFiles = vi.fn(async () => ({ files: [], folders: [] }));
}

interface HostView {
  onContentChange(content: string): void;
  flushSave(): void;
  onRename(title: string): Promise<void>;
  onClose(): Promise<void>;
  onDelete(): Promise<void>;
  closeSelf(): void;
  loadError: () => string | null;
  entry: () => { document: { filePath: string; displayTitle: string; mtime?: number }; isDirty: boolean } | null;
}

const PAYLOAD: DocumentStandaloneTabPayload = {
  docKey: 'k1',
  scope: 'document_store',
  mountPoint: 'Lib',
  filePath: 'notes.md',
  displayTitle: 'Notes',
};

async function flush(): Promise<void> {
  for (let i = 0; i < 12; i++) await Promise.resolve();
}

function render(
  api: FakeApi,
  handle: ReturnType<typeof fakeHandle>,
): { fixture: ComponentFixture<StandaloneDocumentView>; view: HostView } {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [StandaloneDocumentView],
    providers: [
      { provide: StandaloneDocumentApi, useValue: api },
      { provide: WORKSPACE_HANDLE, useValue: handle },
      { provide: WORKSPACE_TAB_ID, useValue: 'tab-1' },
    ],
  });
  // Replace the template so the real ProseMirror DocumentPane is not mounted —
  // the mechanics under test are the view's inline autosave/absorb/409 logic.
  TestBed.overrideComponent(StandaloneDocumentView, { set: { template: '<span></span>', imports: [] } });
  const fixture = TestBed.createComponent(StandaloneDocumentView);
  fixture.componentRef.setInput('payload', PAYLOAD);
  fixture.detectChanges(); // fires ngOnInit → the mount open
  return { fixture, view: fixture.componentInstance as unknown as HostView };
}

function fakeHandle() {
  return {
    openTab: vi.fn(() => 'tab-1'),
    closeTab: vi.fn(),
    refreshTab: vi.fn(),
  };
}

describe('StandaloneDocumentView', () => {
  // Dogfood #97. The workspace mounts this view inside `qt-tab-view`, an
  // unstyled custom element — i.e. `display: inline`, with no flex context.
  // A `flex-1` host therefore collapses to content height and source mode's
  // `h-full` textarea rendered 77px tall inside a 788px pane. v4 has no
  // intervening box at all (`TabView.tsx` renders providers only), so its
  // `flex flex-col h-full` pane measures against `.qt-tab-pane` directly;
  // `h-full` here reproduces that, the way the Salon's `block h-full` already
  // does. Guarding the host class is the only reachable pin — jsdom computes
  // no layout, so a height assertion would be vacuous.
  it('fills the tab pane by height, not by flex-grow (the inline tab host has no flex context)', () => {
    const { fixture } = render(new FakeApi(), fakeHandle());
    const hostClass = (fixture.nativeElement as HTMLElement).className;
    expect(hostClass).toContain('h-full');
    expect(hostClass).not.toContain('flex-1');
  });

  it('opens the document on mount and refreshes the tab with the server filePath', async () => {
    const api = new FakeApi();
    const handle = fakeHandle();
    const { view } = render(api, handle);
    await flush();

    expect(api.open).toHaveBeenCalledWith({
      filePath: 'notes.md',
      title: 'Notes',
      scope: 'document_store',
      mountPoint: 'Lib',
      targetFolder: undefined,
    });
    expect(view.entry()?.document.filePath).toBe('notes.md');
    // Payload refresh carries the server-picked filePath under the same docKey.
    expect(handle.refreshTab).toHaveBeenCalledWith(
      'document-standalone',
      expect.objectContaining({ docKey: 'k1', filePath: 'notes.md', displayTitle: 'Notes' }),
      'Notes',
    );
  });

  it('a failed open surfaces the load error', async () => {
    const api = new FakeApi();
    api.open.mockRejectedValueOnce(new Error('File not found'));
    const { view } = render(api, fakeHandle());
    await flush();
    expect(view.loadError()).toBe('File not found');
  });

  it('absorbs the first post-load re-serialization as baseline (not an edit)', async () => {
    vi.useFakeTimers();
    const api = new FakeApi();
    const { view } = render(api, fakeHandle());
    await flush();

    // The rich editor re-serializes the loaded markdown once after mount; that
    // first change must be absorbed as baseline — not dirty, no autosave armed.
    view.onContentChange('# Notes\n');
    expect(view.entry()?.isDirty).toBe(false);
    vi.advanceTimersByTime(60000);
    await flush();
    expect(api.write).not.toHaveBeenCalled();
  });

  it('a real edit arms the 30s autosave debounce and writes on expiry', async () => {
    vi.useFakeTimers();
    const api = new FakeApi();
    const { view } = render(api, fakeHandle());
    await flush();

    view.onContentChange('# Notes\n'); // absorbed baseline
    view.onContentChange('# Notes\n\nedited');
    expect(view.entry()?.isDirty).toBe(true);

    // Not yet — the debounce hasn't elapsed.
    vi.advanceTimersByTime(29999);
    await flush();
    expect(api.write).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    await flush();
    expect(api.write).toHaveBeenCalledTimes(1);
    expect(api.write).toHaveBeenCalledWith(
      expect.objectContaining({ filePath: 'notes.md', content: '# Notes\n\nedited', mtime: 100 }),
    );
    expect(view.entry()?.isDirty).toBe(false);
    expect(view.entry()?.document.mtime).toBe(101); // the saved mtime
  });

  it('a subsequent edit re-arms the debounce (only the last write fires)', async () => {
    vi.useFakeTimers();
    const api = new FakeApi();
    const { view } = render(api, fakeHandle());
    await flush();

    view.onContentChange('# Notes\n'); // absorbed
    view.onContentChange('first');
    vi.advanceTimersByTime(20000);
    view.onContentChange('second'); // re-arms
    vi.advanceTimersByTime(20000);
    await flush();
    expect(api.write).not.toHaveBeenCalled(); // 20s < 30s since the re-arm
    vi.advanceTimersByTime(10000);
    await flush();
    expect(api.write).toHaveBeenCalledTimes(1);
    expect(api.write).toHaveBeenCalledWith(expect.objectContaining({ content: 'second' }));
  });

  it('flush-on-blur saves immediately and cancels the pending debounce', async () => {
    vi.useFakeTimers();
    const api = new FakeApi();
    const { view } = render(api, fakeHandle());
    await flush();
    view.onContentChange('# Notes\n');
    view.onContentChange('edited');
    view.flushSave();
    await flush();
    expect(api.write).toHaveBeenCalledTimes(1);
    // The debounce was cleared — advancing does not double-write.
    vi.advanceTimersByTime(60000);
    await flush();
    expect(api.write).toHaveBeenCalledTimes(1);
  });

  it('a 409 conflict with local edits reloads mtime and keeps the unsaved content', async () => {
    vi.useFakeTimers();
    const api = new FakeApi();
    api.writeOutcome = { kind: 'conflict' };
    const { view } = render(api, fakeHandle());
    await flush();
    view.onContentChange('# Notes\n'); // absorbed
    view.onContentChange('local edit');
    view.flushSave();
    await flush();

    expect(api.write).toHaveBeenCalledTimes(1);
    // The conflict re-reads the document to refresh mtime.
    expect(api.read).toHaveBeenCalledTimes(1);
    // Local edits are preserved (content not overwritten by disk), mtime refreshed.
    expect(view.entry()?.document.filePath).toBe('notes.md');
    expect(view.entry()?.document.mtime).toBe(200);
  });

  it('rename updates the filePath/title but keeps the docKey stable', async () => {
    const api = new FakeApi();
    const handle = fakeHandle();
    const { view } = render(api, handle);
    await flush();
    handle.refreshTab.mockClear();

    await view.onRename('Renamed');
    expect(api.rename).toHaveBeenCalledWith(
      expect.objectContaining({ filePath: 'notes.md', newTitle: 'Renamed' }),
    );
    expect(view.entry()?.document.filePath).toBe('Renamed.md');
    expect(view.entry()?.document.displayTitle).toBe('Renamed');
    // Same docKey; filePath refreshed.
    expect(handle.refreshTab).toHaveBeenCalledWith(
      'document-standalone',
      expect.objectContaining({ docKey: 'k1', filePath: 'Renamed.md', displayTitle: 'Renamed' }),
      'Renamed',
    );
  });

  it('rename to the same title is a no-op', async () => {
    const api = new FakeApi();
    const { view } = render(api, fakeHandle());
    await flush();
    await view.onRename('  Notes  ');
    expect(api.rename).not.toHaveBeenCalled();
  });

  it('delete removes the file then closes the tab', async () => {
    const api = new FakeApi();
    const handle = fakeHandle();
    const { view } = render(api, handle);
    await flush();
    await view.onDelete();
    expect(api.remove).toHaveBeenCalledWith(
      expect.objectContaining({ filePath: 'notes.md', scope: 'document_store', mountPoint: 'Lib' }),
    );
    expect(handle.closeTab).toHaveBeenCalledWith('tab-1');
  });

  it('a failed delete does NOT close the tab', async () => {
    const api = new FakeApi();
    api.remove.mockRejectedValueOnce(new Error('locked'));
    const handle = fakeHandle();
    const { view } = render(api, handle);
    await flush();
    await view.onDelete();
    expect(handle.closeTab).not.toHaveBeenCalled();
  });

  it('close flushes a pending save then closes the tab', async () => {
    vi.useFakeTimers();
    const api = new FakeApi();
    const handle = fakeHandle();
    const { view } = render(api, handle);
    await flush();
    view.onContentChange('# Notes\n');
    view.onContentChange('edited');
    await view.onClose();
    await flush();
    expect(api.write).toHaveBeenCalledTimes(1);
    expect(handle.closeTab).toHaveBeenCalledWith('tab-1');
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  beforeEach(() => {
    vi.useRealTimers();
  });
});
