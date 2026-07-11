import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { CollapsibleCard } from '../../ui/collapsible-card';
import { Icon } from '../../ui/icon';
import { formatBytes } from '../../ui/format-bytes';
import type { DocumentStoreSummary } from '../../core/core-contract';

/**
 * The group "Scriptorium" card (v4 `GroupLinkedStoresCard.tsx`): the linked
 * document stores with a mountType pill, "(disabled)" marker, file count +
 * formatted bytes, and an X to unlink.
 *
 * DEFERRED LOUDLY: the "Link Document Store" picker needs the GLOBAL
 * mount-points listing (v4 `GET /api/v1/mount-points`), which is NOT a ported
 * dispatch surface this round (it belongs to the future Scriptorium vertical).
 * The `groupMountPointList` variant returns only the LINKED stores. So the Link
 * button renders disabled with a v4-register tooltip; unlink + list are live.
 */
@Component({
  selector: 'qt-group-stores-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, Icon],
  template: `
    <qt-collapsible-card title="The Scriptorium" [description]="subtitle()" icon="scriptorium">
      @if (stores().length === 0) {
        <div class="text-center qt-text-secondary py-2">
          <p>No document stores linked.</p>
          <p class="qt-text-small mt-1">
            Link a document store to enable AI-powered file editing and search.
          </p>
        </div>
      } @else {
        <div class="max-h-64 overflow-y-auto space-y-1">
          @for (store of stores(); track store.id) {
            <div
              class="flex items-center justify-between p-3 rounded-lg transition-colors"
              [class.hover:qt-bg-muted]="store.enabled !== false"
              [class.qt-bg-muted]="store.enabled === false"
            >
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <p class="qt-label truncate">{{ store.name }}</p>
                  <span class="qt-text-xs qt-bg-muted px-2 py-0.5 rounded-full flex-shrink-0">{{
                    store.mountType
                  }}</span>
                  @if (store.enabled === false) {
                    <span class="qt-text-xs qt-text-secondary flex-shrink-0">(disabled)</span>
                  }
                </div>
                <p class="qt-text-xs qt-text-secondary mt-1">
                  {{ fileLabel(store) }} &bull; {{ bytesLabel(store) }}
                </p>
              </div>
              <button
                type="button"
                class="ml-2 flex-shrink-0 p-1.5 rounded hover:qt-text-destructive transition-colors disabled:opacity-50"
                title="Unlink document store"
                aria-label="Unlink document store"
                [disabled]="unlinking() === store.id"
                (click)="unlink.emit(store.id)"
              >
                <qt-icon name="close" class="w-4 h-4" />
              </button>
            </div>
          }
        </div>
      }

      <div class="pt-2 mt-2 border-t qt-border-default">
        <button
          type="button"
          class="w-full qt-button qt-button-primary text-sm"
          title="Browsing the full Scriptorium to link a store is not yet available"
          disabled
        >
          Link Document Store
        </button>
      </div>
    </qt-collapsible-card>
  `,
})
export class GroupStoresCard {
  readonly stores = input.required<DocumentStoreSummary[]>();
  readonly unlinking = input<string | null>(null);
  readonly unlink = output<string>();

  protected readonly subtitle = computed(() => {
    const n = this.stores().length;
    return `${n} store${n !== 1 ? 's' : ''} linked`;
  });

  protected fileLabel(store: DocumentStoreSummary): string {
    const n = store.fileCount ?? 0;
    return `${n} file${n !== 1 ? 's' : ''}`;
  }

  protected bytesLabel(store: DocumentStoreSummary): string {
    return formatBytes(store.totalSizeBytes ?? 0);
  }
}
