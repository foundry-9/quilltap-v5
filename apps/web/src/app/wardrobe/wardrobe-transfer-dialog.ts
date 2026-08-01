import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { WardrobeItemDto } from '../core/core-contract';
import { Modal } from '../ui/modal';
import {
  dispatchWardrobe,
  type TransferDestinationsPayload,
  type WardrobeTransferScope,
} from './wardrobe.api';
import { ToastService } from '../ui/toast.service';

export type TransferMode = 'move' | 'copy';

interface DestinationValue {
  scope: WardrobeTransferScope;
  id: string | null;
}

/** v4 `WardrobeTransferDialog.tsx:38-40`. */
export function encodeDestination(scope: WardrobeTransferScope, id: string | null): string {
  return `${scope}:${id ?? ''}`;
}

/** v4 `WardrobeTransferDialog.tsx:42-50`. */
export function decodeDestination(value: string): DestinationValue | null {
  const [scopeRaw, idRaw] = value.split(':', 2);
  if (!scopeRaw) return null;
  if (
    scopeRaw !== 'general' &&
    scopeRaw !== 'project' &&
    scopeRaw !== 'group' &&
    scopeRaw !== 'character'
  ) {
    return null;
  }
  const id = idRaw && idRaw.length > 0 ? idRaw : null;
  return { scope: scopeRaw, id };
}

/**
 * Move / copy a wardrobe item to another tier — a port of v4
 * `components/wardrobe/WardrobeTransferDialog.tsx` over lane P4.9f1's
 * `wardrobeTransferDestinations` / `wardrobeTransferApply` verbs (v4
 * `GET|POST /api/v1/wardrobe/transfers`, `:73`/`:104`).
 *
 * Success raises v4's sentence and closes via `transferred`, which the parent
 * answers by reloading items.
 */
@Component({
  selector: 'qt-wardrobe-transfer-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal
      [title]="title()"
      maxWidth="lg"
      [closeOnBackdrop]="!working()"
      (close)="requestClose()"
    >
      <div class="space-y-3">
        <p class="qt-text-sm">
          {{ mode() === 'move' ? 'Move' : 'Copy' }}
          <span class="font-medium">"{{ item().title }}"</span> to:
        </p>


        @if (loadingDestinations()) {
          <p class="qt-text-sm qt-text-secondary">Loading destinations…</p>
        } @else {
          <div>
            <label for="wardrobe-transfer-destination" class="qt-text-sm qt-text-secondary">
              Destination
            </label>
            <!-- The options load asynchronously, so the selection rides
                 [selected] per option, never [value] on the select (the
                 standing dogfood-#6 rule). -->
            <select
              id="wardrobe-transfer-destination"
              class="qt-select w-full mt-1"
              (change)="selectedDestination.set($any($event.target).value)"
              [disabled]="working()"
            >
              @if (noDestinations()) {
                <option value="" [selected]="selectedDestination() === ''">
                  No destinations available
                </option>
              }

              @if (destinations()?.general?.available) {
                <optgroup label="General">
                  <option
                    [value]="generalValue"
                    [selected]="selectedDestination() === generalValue"
                  >
                    {{ destinations()?.general?.label }}
                  </option>
                </optgroup>
              }

              @if ((destinations()?.projects ?? []).length > 0) {
                <optgroup label="Projects">
                  @for (project of destinations()?.projects ?? []; track project.id) {
                    <option
                      [value]="encode('project', project.id)"
                      [selected]="selectedDestination() === encode('project', project.id)"
                    >
                      {{ project.name }}
                    </option>
                  }
                </optgroup>
              }

              @if ((destinations()?.groups ?? []).length > 0) {
                <optgroup label="Groups">
                  @for (group of destinations()?.groups ?? []; track group.id) {
                    <option
                      [value]="encode('group', group.id)"
                      [selected]="selectedDestination() === encode('group', group.id)"
                    >
                      {{ group.name }}
                    </option>
                  }
                </optgroup>
              }

              @if ((destinations()?.users ?? []).length > 0) {
                <optgroup label="Users">
                  @for (user of destinations()?.users ?? []; track user.id) {
                    <option
                      [value]="encode('character', user.id)"
                      [selected]="selectedDestination() === encode('character', user.id)"
                    >
                      {{ user.name }}
                    </option>
                  }
                </optgroup>
              }
            </select>
          </div>
        }

        <p class="qt-text-xs qt-text-secondary">
          Copy creates a new item ID in the destination. Move keeps the item ID and removes it
          from its current location.
        </p>
      </div>

      <div qt-modal-footer class="flex justify-end gap-2">
        <button
          type="button"
          (click)="closed.emit()"
          class="qt-button-secondary qt-button-sm"
          [disabled]="working()"
        >
          Cancel
        </button>
        <button
          type="button"
          (click)="handleSubmit()"
          class="qt-button-primary qt-button-sm"
          [disabled]="working() || loadingDestinations() || !selection()"
        >
          {{ working() ? (mode() === 'move' ? 'Moving…' : 'Copying…') : submitLabel() }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class WardrobeTransferDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly mode = input.required<TransferMode>();
  readonly item = input.required<WardrobeItemDto>();
  readonly sourceCharacterId = input.required<string>();
  readonly sourceProjectId = input.required<string | null>();

  readonly closed = output<void>();
  readonly transferred = output<void>();

  protected readonly generalValue = encodeDestination('general', null);

  protected readonly loadingDestinations = signal(false);
  protected readonly destinations = signal<TransferDestinationsPayload | null>(null);
  protected readonly selectedDestination = signal('');
  protected readonly working = signal(false);

  private loaded = false;

  constructor() {
    // Load destinations once on mount (v4 :66-90 — the component mounts per
    // open, so the reset-on-open collapses into the initial state here).
    effect(() => {
      if (this.loaded) return;
      this.loaded = true;
      untracked(() => void this.loadDestinations());
    });
  }

  private async loadDestinations(): Promise<void> {
    this.loadingDestinations.set(true);
    try {
      const data = await dispatchWardrobe(this.core, { type: 'wardrobeTransferDestinations' });
      const destinations = data['destinations'] as TransferDestinationsPayload;
      this.destinations.set(destinations);
      // v4 :80-82 — preselect General when available.
      if (destinations.general.available) {
        this.selectedDestination.set(this.generalValue);
      }
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : String(err));
    } finally {
      this.loadingDestinations.set(false);
    }
  }

  protected readonly selection = computed(() => decodeDestination(this.selectedDestination()));

  /** v4 `:97-98`. */
  protected readonly submitLabel = computed(() =>
    this.mode() === 'move' ? 'Move item' : 'Copy item',
  );
  protected readonly title = computed(() =>
    this.mode() === 'move' ? 'Move wardrobe item' : 'Copy wardrobe item',
  );

  /** v4 `:181-185` — the all-empty placeholder option. */
  protected readonly noDestinations = computed(() => {
    const d = this.destinations();
    return (
      !!d &&
      !d.general.available &&
      d.projects.length === 0 &&
      d.groups.length === 0 &&
      d.users.length === 0
    );
  });

  protected encode(scope: WardrobeTransferScope, id: string | null): string {
    return encodeDestination(scope, id);
  }

  /** v4 `:137` — click-outside/Escape closing suspends while working. */
  protected requestClose(): void {
    if (!this.working()) this.closed.emit();
  }

  /** v4 `:100-129` — the POST body (`:107-116`) verbatim: `id` rides only
   *  when the selection has one. */
  protected async handleSubmit(): Promise<void> {
    const selection = this.selection();
    if (!selection) return;
    this.working.set(true);
    try {
      await dispatchWardrobe(this.core, {
        type: 'wardrobeTransferApply',
        action: this.mode(),
        itemId: this.item().id,
        sourceCharacterId: this.sourceCharacterId(),
        sourceProjectId: this.sourceProjectId(),
        destination: {
          scope: selection.scope,
          ...(selection.id ? { id: selection.id } : {}),
        },
      });
      // v4 `:122` — the sentence names the mode and the item.
      this.toasts.showSuccess(
        this.mode() === 'move' ? `Moved "${this.item().title}"` : `Copied "${this.item().title}"`,
      );
      this.transferred.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : String(err));
    } finally {
      this.working.set(false);
    }
  }
}
