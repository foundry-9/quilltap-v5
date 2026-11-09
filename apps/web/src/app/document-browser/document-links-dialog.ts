import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../core/core-client';
import type { DocMountLinkLocation } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { fetchDocumentFileLinks } from './document-store.api';
import { mountPointKeys } from './document-store.keys';

/**
 * "Where else this appears" — v4 `DocumentLinksDialog`
 * (`components/document-browser/DocumentFileDialogs.tsx`, `c34986421`; P4.D387
 * item 10). The locations are read (`?action=links`, `mountFileLinks`) only
 * while a path is asked about; `Go there` closes the dialog, then reveals the
 * location (this store's folder, or another store).
 */
@Component({
  selector: 'qt-document-links-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    @if (path() !== null) {
      <qt-modal title="Where else this appears" maxWidth="lg" (close)="closed.emit()">
        <p class="qt-text-xs qt-text-secondary mb-3">{{ note() }}</p>
        @if (links.isLoading()) {
          <div class="text-sm qt-text-secondary">Consulting the ledger…</div>
        } @else if (links.error()) {
          <div class="text-sm qt-text-destructive">{{ errorText() }}</div>
        } @else {
          <ul class="space-y-1" data-testid="document-links-list">
            @for (loc of locations(); track loc.linkId) {
              <li class="qt-card py-2 px-3 flex items-center gap-2">
                <div class="flex-1 min-w-0">
                  <div class="text-sm text-foreground truncate" [attr.title]="loc.relativePath">
                    {{ loc.relativePath }}
                  </div>
                  <div class="qt-text-xs qt-text-secondary truncate">{{ loc.mountPointName }}</div>
                </div>
                @if (loc.self) {
                  <span class="qt-badge-disabled">This one</span>
                }
                @if (loc.grouped && !loc.self) {
                  <span class="qt-badge-info">Hard-linked</span>
                }
                @if (!loc.self) {
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm"
                    (click)="goThere(loc)"
                  >
                    Go there
                  </button>
                }
              </li>
            }
          </ul>
        }
        <div qt-modal-footer class="flex justify-end">
          <button type="button" class="qt-button-primary qt-button-sm" (click)="closed.emit()">
            Close
          </button>
        </div>
      </qt-modal>
    }
  `,
})
export class DocumentLinksDialog {
  private readonly core = inject(CoreClient);

  readonly storeId = input.required<string>();
  /** The file asked about; null closes the dialog. */
  readonly path = input<string | null>(null);
  readonly closed = output<void>();
  /** Go to a location (this store's folder, or another store). */
  readonly reveal = output<DocMountLinkLocation>();

  protected readonly links = injectQuery(() => ({
    queryKey: mountPointKeys.links(this.storeId(), this.path() ?? ''),
    queryFn: () => fetchDocumentFileLinks(this.core, this.storeId(), this.path() ?? ''),
    enabled: Boolean(this.path()),
  }));

  protected readonly locations = computed(() => this.links.data()?.locations ?? []);
  protected readonly note = computed(() =>
    this.locations().some((l) => l.grouped && !l.self)
      ? 'Hard-linked copies are one file: an edit through any of them reaches all. The others merely share identical contents and part ways at the next edit.'
      : 'These share identical contents only; each goes its own way at its next edit.',
  );
  protected readonly errorText = computed(() =>
    coreErrorMessage(this.links.error(), 'The locations would not load'),
  );

  protected goThere(loc: DocMountLinkLocation): void {
    this.closed.emit();
    this.reveal.emit(loc);
  }
}
