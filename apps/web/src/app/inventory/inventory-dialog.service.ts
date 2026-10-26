import { Injectable, computed, signal } from '@angular/core';

/**
 * The optional context passed to `open()` (v4 `components/providers/
 * inventory-dialog-provider.tsx:17-26`): a character to preselect, the chat
 * whose Carrying-now pane to show, a project or group whose inventory to start
 * on.
 */
export interface InventoryDialogContext {
  characterId?: string;
  chatId?: string;
  projectId?: string;
  groupId?: string;
}

/**
 * The global Inventory dialog service — v4's `InventoryDialogProvider` as a
 * root-provided signal service (the `WardrobeDialogService` precedent). Any
 * component can summon the dialog via `open()`; the host
 * (`qt-inventory-control-dialog`, mounted once in the shell) renders it while
 * `isOpen`.
 *
 * `remountKey` is v4's inner-component key (`inventory-control-dialog.tsx:86`):
 * `${characterId ?? ''}|${chatId ?? ''}|${projectId ?? ''}|${groupId ?? ''}`.
 * A context change REMOUNTS the inner dialog and discards its state.
 *
 * DIVERGENCE (recorded, as the wardrobe's): v4's optional accessor (and the
 * provider-missing throw, `:48-52`) collapse — a root-provided service is
 * always present.
 */
@Injectable({ providedIn: 'root' })
export class InventoryDialogService {
  private readonly _isOpen = signal(false);
  private readonly _context = signal<InventoryDialogContext | null>(null);

  readonly isOpen = this._isOpen.asReadonly();
  readonly context = this._context.asReadonly();

  /** v4 `inventory-control-dialog.tsx:86` — the inner remount key. */
  readonly remountKey = computed(() => {
    const ctx = this._context();
    return `${ctx?.characterId ?? ''}|${ctx?.chatId ?? ''}|${ctx?.projectId ?? ''}|${ctx?.groupId ?? ''}`;
  });

  /** v4 `inventory-dialog-provider.tsx:38-41`. */
  open(ctx?: InventoryDialogContext): void {
    this._context.set(ctx ?? null);
    this._isOpen.set(true);
  }

  /** v4 `:42-44` — close keeps the last context, exactly as v4's `close` only flips `isOpen`. */
  close(): void {
    this._isOpen.set(false);
  }
}
