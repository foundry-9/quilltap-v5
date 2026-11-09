import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { QueryClient, injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import {
  DestinationPickerDialog,
  type DestinationMode,
} from '../item-browser/destination-picker-dialog';
import { canLinkBetween, capabilitiesOf, type BrowserStore } from './document-actions';
import {
  createDocumentFolder,
  fetchDocumentListing,
  fetchStoreCatalogue,
} from './document-store.api';
import { mountPointKeys } from './document-store.keys';

/** Where a transfer goes: a store and a folder in it (v4 `DocumentDestination`). */
export interface DocumentDestination {
  mountPointId: string;
  folder: string;
}

/** v4 `NOTES` (`DocumentDestinationDialog.tsx:66-70`). */
export const DESTINATION_NOTES: Record<DestinationMode, string> = {
  move: 'Choose a store and a folder. A file of the same name already there stops the move.',
  copy: 'Choose a store and a folder. The copy is independent of the original.',
  link: 'A hard link is the same file in two places: an edit through either reaches both.',
};

/** v4 `useStoreCatalogue(enabled)` — the stores a destination may be chosen from. */
export function injectStoreCatalogue(enabled: () => boolean) {
  const core = inject(CoreClient);
  return injectQuery(() => ({
    queryKey: mountPointKeys.list(),
    queryFn: () => fetchStoreCatalogue(core),
    enabled: enabled(),
  }));
}

/**
 * Move / Copy / Hard link to any folder of this store or another that can
 * take it — a port of v4 `components/document-browser/
 * DocumentDestinationDialog.tsx` (`c34986421`, pin `624d0d25c`; P4.D387 item
 * 11) over `BROWSER`'s `DestinationPickerDialog`, with the store `<select>`
 * in its container slot.
 *
 * The eligible stores: for a link, `canLinkBetween(source, store)`; else any
 * enabled store that can take a move in. The destination: the chosen store
 * while it stays eligible, else this store, else the first. The folder the
 * items already share is the picker's "here now" only in this store.
 */
@Component({
  selector: 'qt-document-destination-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DestinationPickerDialog],
  template: `
    <qt-destination-picker-dialog
      [mode]="mode()"
      [subject]="subject()"
      [containerKey]="destId()"
      [folders]="destId() ? folders() : []"
      [loading]="listing.isLoading()"
      rootLabel="Top of the store"
      [createFolder]="createFolder"
      [creating]="creating()"
      [currentFolder]="destId() === source().id ? currentFolder() : null"
      [working]="working()"
      [note]="note()"
      (closed)="closed.emit()"
      (submitted)="onSubmitted($event)"
    >
      <div qtDestinationContainer class="mb-3">
        <label for="document-destination-store" class="qt-label block mb-1">Store</label>
        <select
          id="document-destination-store"
          class="qt-select w-full"
          [value]="destId()"
          (change)="chosen.set($any($event.target).value)"
          [disabled]="working() || stores.isLoading()"
        >
          @if (eligible().length === 0) {
            <option value="">
              {{ stores.isLoading() ? 'Loading stores…' : 'No store can take this' }}
            </option>
          }
          @for (s of eligible(); track s.id) {
            <option [value]="s.id" [selected]="s.id === destId()">
              {{ s.name }}{{ s.id === source().id ? ' (this store)' : '' }}
            </option>
          }
        </select>
      </div>
    </qt-destination-picker-dialog>
  `,
})
export class DocumentDestinationDialog {
  private readonly core = inject(CoreClient);
  private readonly queryClient = inject(QueryClient);

  readonly mode = input.required<DestinationMode>();
  readonly source = input.required<BrowserStore>();
  /** What is sent, for the title (`"notes.md"`, `3 files`, `folder "Drafts"`). */
  readonly subject = input.required<string>();
  /** The folder the items share in the source store (disabled for a move there), or null. */
  readonly currentFolder = input<string | null>(null);
  readonly working = input(false);
  readonly closed = output<void>();
  readonly submitted = output<DocumentDestination>();

  protected readonly stores = injectStoreCatalogue(() => true);
  protected readonly chosen = signal<string | null>(null);
  protected readonly creating = signal(false);

  protected readonly eligible = computed(() => {
    const source = this.source();
    const mode = this.mode();
    return (this.stores.data() ?? []).filter((s) =>
      mode === 'link'
        ? canLinkBetween(source, s)
        : s.enabled !== false && capabilitiesOf(s).canMoveIn,
    );
  });

  protected readonly destId = computed(() => {
    const eligible = this.eligible();
    const chosen = this.chosen();
    if (chosen && eligible.some((s) => s.id === chosen)) return chosen;
    return eligible.find((s) => s.id === this.source().id)?.id ?? eligible[0]?.id ?? '';
  });

  protected readonly listing = injectQuery(() => ({
    queryKey: mountPointKeys.files(this.destId()),
    queryFn: () => fetchDocumentListing(this.core, this.destId()),
    enabled: this.destId() !== '',
  }));

  protected readonly folders = computed(() =>
    [...(this.listing.data()?.folders ?? [])]
      .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
      .map((path) => ({ path })),
  );

  protected readonly note = computed(() => DESTINATION_NOTES[this.mode()]);

  protected readonly createFolder = async (path: string): Promise<{ path: string }> => {
    const destId = this.destId();
    this.creating.set(true);
    try {
      await createDocumentFolder(this.core, destId, path);
      await this.queryClient.invalidateQueries({ queryKey: mountPointKeys.store(destId) });
      return { path };
    } finally {
      this.creating.set(false);
    }
  };

  protected onSubmitted(folder: string): void {
    const destId = this.destId();
    if (destId) this.submitted.emit({ mountPointId: destId, folder });
  }
}
