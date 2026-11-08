import { signal } from '@angular/core';

/**
 * One open / close state per item kind — the shape v4's generic
 * `components/item-browser/item-dialog-provider.tsx` gives both kinds at the
 * pin `624d0d25c` (NEW at `600c2906f`; the wardrobe and Inventory providers
 * became thin wrappers over it). Here it is a base class the two root
 * services extend; their public APIs and `remountKey`s are unchanged.
 *
 * v4's doc, carried: `open(ctx?)` sets the context and shows the dialog;
 * `close()` only hides it, keeping the last context.
 *
 * DIVERGENCE (recorded, as before): v4's optional accessor and its
 * provider-missing throw collapse — a root-provided service is always
 * present.
 */
export abstract class ItemDialogState<C> {
  private readonly _isOpen = signal(false);
  private readonly _context = signal<C | null>(null);

  readonly isOpen = this._isOpen.asReadonly();
  readonly context = this._context.asReadonly();

  open(ctx?: C): void {
    this._context.set(ctx ?? null);
    this._isOpen.set(true);
  }

  /** Close keeps the last context, exactly as v4's `close` only flips `isOpen`. */
  close(): void {
    this._isOpen.set(false);
  }
}
