import { Injectable, computed } from '@angular/core';

import { ItemDialogState } from '../item-browser/item-dialog.service';

/**
 * Shape of the optional context passed to `open()` so the dialog can preselect
 * a character and surface chat-scoped controls when invoked from the Salon
 * (v4 `components/providers/wardrobe-dialog-provider.tsx:23-28`).
 */
export interface WardrobeDialogContext {
  /** Preselect this character in the dialog header dropdown. */
  characterId?: string;
  /** When set, the dialog renders the "wearing now" column for this chat. */
  chatId?: string;
}

/**
 * The global wardrobe-dialog service — v4's `WardrobeDialogProvider`
 * (`components/providers/wardrobe-dialog-provider.tsx`) as a root-provided
 * signal service. Any component can summon the dialog via `open()`, passing
 * optional `characterId` / `chatId` context; the host
 * (`qt-wardrobe-control-dialog`, mounted once in the shell — v4
 * `app-layout.tsx:126-138`) renders the dialog while `isOpen`.
 *
 * `remountKey` is v4's inner-component `key`
 * (`wardrobe-control-dialog.tsx:92`): `${characterId}|${chatId}` with the
 * `'auto'` / `'no-chat'` fallbacks. A context change REMOUNTS the inner dialog
 * and discards all staged state — the host reproduces that deliberately via a
 * keyed `@for` track; do not "optimize" it into an in-place update.
 *
 * DIVERGENCE (deliberate, recorded per the order): v4 also exports
 * `useWardrobeDialogOptional` so entry buttons outside the provider tree can
 * render `disabled={!wardrobeDialog}`. In v5 a root-provided service is always
 * present, so the optional accessor and the disabled arm collapse.
 */
@Injectable({ providedIn: 'root' })
export class WardrobeDialogService extends ItemDialogState<WardrobeDialogContext> {
  /** v4 `wardrobe-control-dialog.tsx:92` — the inner remount key. */
  readonly remountKey = computed(
    () => `${this.context()?.characterId ?? 'auto'}|${this.context()?.chatId ?? 'no-chat'}`,
  );
}
