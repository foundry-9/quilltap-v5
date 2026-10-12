import { ChangeDetectionStrategy, Component, input, output, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { WardrobeItemDto } from '../core/core-contract';
import type { WardrobeContainer } from './wardrobe-container';
import { WardrobeImageViewer } from './wardrobe-image-viewer';
import { createWardrobeItemActions } from './wardrobe-item-actions';
import { WardrobeItemEditor } from './wardrobe-item-editor';
import type { WardrobeListData } from './wardrobe-list-data';
import { WardrobeItemOverlays } from './wardrobe-item-overlays';
import { WardrobeTransferDialog } from './wardrobe-transfer-dialog';

/**
 * P4.D285 — v4 `WardrobeBrowser.tsx:806-892` `WardrobeItemOverlays`
 * (`4a38222e6`): which overlay mounts for which actions state, and the
 * transfer dialog's origin-aware `source` / `excludeDestination` (`:869-882`).
 * The three children are stubbed by selector — their own specs pin them.
 */

@Component({ selector: 'qt-wardrobe-item-editor', template: '' })
class EditorStub {
  readonly characterId = input<string | null>(null);
  readonly item = input<WardrobeItemDto | null>(null);
  readonly projectId = input<string | null>(null);
  readonly container = input<WardrobeContainer | null>(null);
  readonly containerLabel = input<string | undefined>(undefined);
  readonly initialMode = input<string | undefined>(undefined);
  readonly initialComponentItemIds = input<string[] | undefined>(undefined);
  readonly autoFocusTitle = input(false);
  readonly closed = output<void>();
  readonly saved = output<void>();
  readonly imageChanged = output<void>();
}

@Component({ selector: 'qt-wardrobe-transfer-dialog', template: '' })
class TransferStub {
  readonly mode = input<string>('move');
  readonly item = input<WardrobeItemDto | null>(null);
  readonly sourceCharacterId = input<string | null>(null);
  readonly sourceProjectId = input<string | null>(null);
  readonly source = input<WardrobeContainer | null>(null);
  readonly excludeDestination = input<WardrobeContainer | null>(null);
  readonly closed = output<void>();
  readonly transferred = output<void>();
}

@Component({ selector: 'qt-wardrobe-image-viewer', template: '' })
class ViewerStub {
  readonly itemId = input('');
  readonly itemTitle = input('');
  readonly container = input<WardrobeContainer | null>(null);
  readonly fileId = input('');
  readonly closed = output<void>();
}

@Component({
  selector: 'qt-overlays-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [WardrobeItemOverlays],
  template: `<qt-wardrobe-item-overlays
    [container]="container()"
    containerLabel="Alice"
    [data]="data"
    [actions]="actions"
  />`,
})
class Host {
  readonly container = signal<WardrobeContainer | null>(null);
  data!: WardrobeListData;
  actions!: ReturnType<typeof createWardrobeItemActions>;
}

const item = (id: string, extra: Partial<WardrobeItemDto> = {}): WardrobeItemDto => ({
  id,
  title: id,
  types: ['top'],
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
  ...extra,
});

function render(container: WardrobeContainer) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  TestBed.overrideComponent(WardrobeItemOverlays, {
    remove: { imports: [WardrobeItemEditor, WardrobeTransferDialog, WardrobeImageViewer] },
    add: { imports: [EditorStub, TransferStub, ViewerStub] },
  });
  const fixture = TestBed.createComponent(Host);
  const reloadLists = vi.fn(async () => undefined);
  const actions = createWardrobeItemActions({
    core: {} as CoreClient,
    toasts: { showError: vi.fn(), showSuccess: vi.fn() },
    container: () => container,
    listItems: () => [],
    confirm: () => true,
    reloadLists,
  });
  fixture.componentInstance.container.set(container);
  fixture.componentInstance.data = {
    listItems: signal([]),
    resolutionPool: signal([]),
    characterItems: signal([]),
    loading: signal(false),
    projectId: signal('proj-9'),
    canManage: () => true,
    reload: async () => undefined,
  };
  fixture.componentInstance.actions = actions;
  fixture.detectChanges();
  const find = <T>(type: new (...a: never[]) => T): T | null => {
    const de = fixture.debugElement.query((d) => d.componentInstance instanceof type);
    return de ? (de.componentInstance as T) : null;
  };
  return { fixture, actions, find, reloadLists };
}

const ALICE: WardrobeContainer = { scope: 'character', id: 'alice' };

describe('WardrobeItemOverlays (v4 `WardrobeBrowser.tsx:806-892`)', () => {
  it('mounts nothing until an overlay is asked for', () => {
    const { find } = render(ALICE);
    expect(find(EditorStub)).toBeNull();
    expect(find(TransferStub)).toBeNull();
    expect(find(ViewerStub)).toBeNull();
  });

  it('the editor: edit vs create, the bundle seeds, the character and project of the view', () => {
    const { fixture, actions, find } = render(ALICE);
    actions.openCreate('bundle', ['a', 'b'], { folder: 'Formal' });
    fixture.detectChanges();
    const editor = find(EditorStub)!;
    expect(editor.characterId()).toBe('alice');
    expect(editor.item()).toBeNull();
    expect(editor.projectId()).toBe('proj-9');
    expect(editor.containerLabel()).toBe('Alice');
    expect(editor.initialMode()).toBe('bundle');
    expect(editor.initialComponentItemIds()).toEqual(['a', 'b']);
    expect(editor.autoFocusTitle()).toBe(true);
    const coat = item('coat', { characterId: 'alice' });
    actions.openEditor(coat);
    fixture.detectChanges();
    expect(find(EditorStub)!.item()).toBe(coat);
    expect(find(EditorStub)!.initialMode()).toBeUndefined();
  });

  it('a save closes the editor and refreshes', async () => {
    const { fixture, actions, find, reloadLists } = render(ALICE);
    actions.openCreate('single');
    fixture.detectChanges();
    find(EditorStub)!.saved.emit();
    await Promise.resolve();
    expect(actions.editor()).toBeNull();
    expect(reloadLists).toHaveBeenCalled();
  });

  it('character view: a borrowed item names its origin wardrobe as source AND exclusion', () => {
    const { fixture, actions, find } = render(ALICE);
    actions.move(item('cape', { origin: { scope: 'project', id: 'p1', name: 'B' } }));
    fixture.detectChanges();
    const t = find(TransferStub)!;
    expect(t.source()).toEqual({ scope: 'project', id: 'p1' });
    expect(t.excludeDestination()).toEqual({ scope: 'project', id: 'p1' });
    expect(t.sourceCharacterId()).toBe('alice');
  });

  it('character view: an origin-less own item is excluded but left for the server to probe', () => {
    const { fixture, actions, find } = render(ALICE);
    actions.copy(item('coat', { characterId: 'alice' }));
    fixture.detectChanges();
    const t = find(TransferStub)!;
    expect(t.mode()).toBe('copy');
    expect(t.source()).toBeNull();
    expect(t.excludeDestination()).toEqual({ scope: 'character', id: 'alice' });
  });

  it('character view: an origin-less merged item has neither', () => {
    const { fixture, actions, find } = render(ALICE);
    actions.move(item('watch'));
    fixture.detectChanges();
    expect(find(TransferStub)!.source()).toBeNull();
    expect(find(TransferStub)!.excludeDestination()).toBeNull();
  });

  it('a shared view names the browsed container both ways', () => {
    const PROJECT: WardrobeContainer = { scope: 'project', id: 'p1' };
    const { fixture, actions, find } = render(PROJECT);
    actions.move(item('cape'));
    fixture.detectChanges();
    expect(find(TransferStub)!.source()).toEqual(PROJECT);
    expect(find(TransferStub)!.excludeDestination()).toEqual(PROJECT);
    expect(find(TransferStub)!.sourceCharacterId()).toBeNull();
  });

  it('the viewer mounts for a picture, addressed through the item’s home', () => {
    const { fixture, actions, find } = render(ALICE);
    actions.openImage(item('cape', { imageFileId: 'f1', origin: { scope: 'group', id: 'g1', name: 'S' } }));
    fixture.detectChanges();
    const v = find(ViewerStub)!;
    expect(v.fileId()).toBe('f1');
    expect(v.container()).toEqual({ scope: 'group', id: 'g1' });
    v.closed.emit();
    expect(actions.imageView()).toBeNull();
  });
});
