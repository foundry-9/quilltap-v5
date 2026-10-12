import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { WardrobeContainer } from './wardrobe-container';
import { WardrobeImageViewer } from './wardrobe-image-viewer';
import type { WardrobeItemActions } from './wardrobe-item-actions';
import { WardrobeItemEditor } from './wardrobe-item-editor';
import type { WardrobeListData } from './wardrobe-list-data';
import { WardrobeTransferDialog } from './wardrobe-transfer-dialog';

/**
 * The overlays a browser's row actions open, stacked above it — a port of v4
 * `WardrobeBrowser.tsx:806-892` `WardrobeItemOverlays` (`4a38222e6`). A
 * component of its own so both hosts (the wardrobe dialog and the Prospero
 * project card) mount the same three: the full-screen picture, the item
 * editor, and the move / copy dialog.
 *
 * The transfer dialog's `source` / `excludeDestination` are v4's origin-aware
 * values (`:869-882`): name the item's home as the source whenever it is
 * known — the browsed shared container, or in the character view the wardrobe
 * the item's origin names. Only an origin-less merged item falls back to the
 * server's probe across the character's tiers. (v5 `main` passed `null` in
 * every character view and excluded only a character-owned item's vault.)
 *
 * **HANDOFFs to the unifier (status-log, P4.D285 unit 6):** the editor's
 * `[initialFolder]` (C3 §6, P4.D286's new input — the browsed folder on
 * create) and the viewer's `(removed)` output (bug 194, P4.D286's own
 * §R.10(m) handoff for this host) bind once P4.D286's components carry them.
 * The editor's `isShared` binding is gone (v4 dropped the prop; P4.D286
 * removes the input) — its default was the `false` v5 passed.
 */
@Component({
  selector: 'qt-wardrobe-item-overlays',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [WardrobeImageViewer, WardrobeItemEditor, WardrobeTransferDialog],
  template: `
    <!-- A row's picture, full screen — above the dialog and the editor -->
    @if (actions().imageView(); as view) {
      @if (view.item.imageFileId; as fileId) {
        <qt-wardrobe-image-viewer
          [itemId]="view.item.id"
          [itemTitle]="view.item.title"
          [container]="view.container"
          [fileId]="fileId"
          (closed)="actions().closeImage()"
        />
      }
    }

    @if (actions().editor(); as editor) {
      @if (container(); as c) {
        <qt-wardrobe-item-editor
          [characterId]="characterId()"
          [item]="editor.kind === 'edit' ? editor.item : null"
          [projectId]="data().projectId()"
          [container]="c"
          [containerLabel]="containerLabel()"
          [initialMode]="editor.kind === 'create' ? editor.mode : undefined"
          [initialComponentItemIds]="
            editor.kind === 'create' && editor.mode === 'bundle' ? editor.componentItemIds : undefined
          "
          [autoFocusTitle]="editor.kind === 'create' && editor.mode === 'bundle'"
          (closed)="actions().closeEditor()"
          (saved)="onSaved()"
          (imageChanged)="actions().refresh()"
        />
      }
    }

    @if (actions().transfer(); as transfer) {
      @if (container(); as c) {
        <qt-wardrobe-transfer-dialog
          [mode]="transfer.mode"
          [item]="transfer.item"
          [sourceCharacterId]="characterId()"
          [sourceProjectId]="data().projectId()"
          [source]="transferSource()"
          [excludeDestination]="transferExclude()"
          (closed)="actions().closeTransfer()"
          (transferred)="onTransferred()"
        />
      }
    }
  `,
})
export class WardrobeItemOverlays {
  readonly container = input.required<WardrobeContainer | null>();
  /** Display name of `container`, for the editor's pinned destination note. */
  readonly containerLabel = input('');
  readonly data = input.required<WardrobeListData>();
  readonly actions = input.required<WardrobeItemActions>();

  private readonly isCharacterScope = computed(() => this.container()?.scope === 'character');
  protected readonly characterId = computed(() =>
    this.isCharacterScope() ? (this.container()?.id ?? null) : null,
  );

  /** v4 `:873-875`. */
  protected readonly transferSource = computed<WardrobeContainer | null>(() => {
    const transfer = this.actions().transfer();
    if (!transfer) return null;
    if (!this.isCharacterScope()) return this.container();
    return transfer.item.origin ? this.actions().addressOf(transfer.item) : null;
  });

  /** v4 `:876-882`. */
  protected readonly transferExclude = computed<WardrobeContainer | null>(() => {
    const transfer = this.actions().transfer();
    if (!transfer) return null;
    if (!this.isCharacterScope()) return this.container();
    return transfer.item.origin || transfer.item.characterId
      ? this.actions().addressOf(transfer.item)
      : null;
  });

  protected async onSaved(): Promise<void> {
    this.actions().closeEditor();
    await this.actions().refresh();
  }

  protected async onTransferred(): Promise<void> {
    this.actions().closeTransfer();
    await this.actions().refresh();
  }
}
