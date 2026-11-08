import { ChangeDetectionStrategy, Component, computed, effect, inject, input, output, signal, untracked } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { WardrobeItemDto } from '../core/core-contract';
import { injectContainerCatalogue } from '../item-browser/container-catalogue';
import { ContainerSelect } from '../item-browser/container-select';
import { Modal } from '../ui/modal';
import { WardrobeComponentModeFieldset, defaultComponentMode } from './wardrobe-component-mode-fieldset';
import { dispatchWardrobe, type WardrobeTransferComponentMode } from './wardrobe.api';
import { isComposite } from './equipped-slots';
import {
  GENERAL_CONTAINER,
  GENERAL_WARDROBE_NAME,
  decodeWardrobeContainer,
  encodeWardrobeContainer,
  type WardrobeContainer,
} from './wardrobe-container';
import { ToastService } from '../ui/toast.service';

export type TransferMode = 'move' | 'copy';

/**
 * Move / copy a wardrobe item to another wardrobe — a character's vault, a
 * group's or project's store, or Quilltap General — a port of v4
 * `components/wardrobe/WardrobeTransferDialog.tsx` at the pin `624d0d25c`.
 * A move keeps the item's id; a copy is a new item. A composite outfit asks
 * what becomes of its components (all or nothing). The destinations are the
 * shared container catalogue (`injectContainerCatalogue` — the
 * `characterList` / `projectList` / `groupList` reads) in the shared
 * `qt-container-select`, the item's own wardrobe left out; General is the
 * natural first choice unless the item already lives there, and needs no
 * availability read (`b2b783b47` retired v4's `GET /wardrobe/transfers`, and
 * this dialog no longer dispatches the destinations read — C12). The
 * transfer itself is `wardrobeTransferApply` (v4 `POST /wardrobe/transfers`).
 *
 * Success raises v4's sentence and closes via `transferred`, which the parent
 * answers by reloading items.
 */
@Component({
  selector: 'qt-wardrobe-transfer-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ContainerSelect, Modal, WardrobeComponentModeFieldset],
  template: `
    <qt-modal
      [title]="title()"
      maxWidth="lg"
      [closeOnBackdrop]="!working()"
      [closeOnEscape]="!working()"
      (close)="requestClose()"
    >
      <div class="space-y-3">
        <p class="text-sm">
          {{ mode() === 'move' ? 'Move' : 'Copy' }}
          <span class="font-medium">"{{ item().title }}"</span> to:
        </p>


        @if (catalogue.loading()) {
          <p class="text-sm qt-text-secondary">Loading destinations…</p>
        } @else {
          <div>
            <label id="wardrobe-transfer-destination-label" for="wardrobe-transfer-destination" class="text-sm qt-text-secondary">
              Destination
            </label>
            <qt-container-select
              id="wardrobe-transfer-destination"
              labelledBy="wardrobe-transfer-destination-label"
              selectClass="qt-select w-full mt-1"
              [value]="selectedDestination()"
              (valueChange)="chosen.set($event)"
              [catalogue]="catalogueLists()"
              [generalName]="generalName"
              placeholder="Choose a wardrobe"
              [exclude]="exclude()"
              [disabled]="working()"
            />
          </div>
        }

        @if (isComposite()) {
          <qt-wardrobe-component-mode-fieldset
            [mode]="mode()"
            [value]="componentMode()"
            (valueChange)="componentMode.set($event)"
            [legend]="componentCountLabel()"
            destination="wardrobe"
            name="wardrobe-transfer-components"
            [disabled]="working()"
          />
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
          [disabled]="working() || catalogue.loading() || !selection()"
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
  /**
   * Character-view source: the server probes the character's reachable tiers
   * (vault → project → groups → General) for the item. Null when the dialog is
   * browsing a shared container — {@link source} is passed instead
   * (v4 `:31-36`).
   */
  readonly sourceCharacterId = input.required<string | null>();
  readonly sourceProjectId = input.required<string | null>();
  /**
   * Explicit source container (shared-container views): the server resolves
   * the item straight from this container, no character probing (v4 `:37-41`).
   */
  readonly source = input<WardrobeContainer | null>(null);
  /**
   * The container the item is known to live in — hidden from the destination
   * list, since moving or copying an item onto itself is refused anyway
   * (v4 `:42-46`).
   */
  readonly excludeDestination = input<WardrobeContainer | null>(null);

  readonly closed = output<void>();
  readonly transferred = output<void>();

  protected readonly catalogue = injectContainerCatalogue();
  protected readonly generalName = GENERAL_WARDROBE_NAME;
  /** v4 `ContainerSelect`'s `catalogue` (plain arrays). */
  protected readonly catalogueLists = computed(() => ({
    characters: this.catalogue.characters(),
    projects: this.catalogue.projects(),
    groups: this.catalogue.groups(),
  }));
  private readonly excludedKey = computed(() => {
    const exclude = this.excludeDestination();
    return exclude ? encodeWardrobeContainer(exclude) : null;
  });
  protected readonly exclude = computed<readonly string[]>(() => {
    const key = this.excludedKey();
    return key ? [key] : [];
  });
  /** The operator's pick; null until one is made (v4 `chosen`). */
  protected readonly chosen = signal<string | null>(null);
  /** v4 `chosen ?? (excludedKey === generalKey ? '' : generalKey)` — General first, unless the item lives there. */
  protected readonly selectedDestination = computed(() => {
    const generalKey = encodeWardrobeContainer(GENERAL_CONTAINER);
    return this.chosen() ?? (this.excludedKey() === generalKey ? '' : generalKey);
  });
  protected readonly working = signal(false);
  /**
   * Composite outfits prompt for their components — all or nothing. A move
   * defaults to moving them along; a copy defaults to copying them, since an
   * outfit that arrives without its pieces is rarely what anyone meant. The
   * component mounts per open, so v4's initial state is read once.
   */
  protected readonly isComposite = computed(() => isComposite(this.item()));
  protected readonly componentMode = signal<WardrobeTransferComponentMode>('copy');

  constructor() {
    effect(() => {
      const mode = this.mode();
      untracked(() => this.componentMode.set(defaultComponentMode(mode)));
    });
  }

  protected readonly selection = computed<WardrobeContainer | null>(() => decodeWardrobeContainer(this.selectedDestination()));

  protected readonly componentCountLabel = computed(() => {
    const count = (this.item().componentItemIds ?? []).length;
    return `This outfit bundles ${count} ${count === 1 ? 'component' : 'components'}`;
  });

  /** v4 `:97-98`. */
  protected readonly submitLabel = computed(() =>
    this.mode() === 'move' ? 'Move item' : 'Copy item',
  );
  protected readonly title = computed(() =>
    this.mode() === 'move' ? 'Move wardrobe item' : 'Copy wardrobe item',
  );

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
      const sourceCharacterId = this.sourceCharacterId();
      const source = this.source();
      await dispatchWardrobe(this.core, {
        type: 'wardrobeTransferApply',
        action: this.mode(),
        itemId: this.item().id,
        // v4 `:163-168` — each of these three rides only when it has a value;
        // an absent `sourceCharacterId` is an ABSENT KEY, not a null, because
        // the server's refine reads presence.
        ...(sourceCharacterId ? { sourceCharacterId } : {}),
        sourceProjectId: this.sourceProjectId(),
        ...(source
          ? { source: { scope: source.scope, ...(source.id ? { id: source.id } : {}) } }
          : {}),
        ...(this.isComposite() ? { components: this.componentMode() } : {}),
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
      // v4 pin: `result.error || \`Failed to ${mode} wardrobe item\`` (the refusal's words, else the fallback).
      this.toasts.showError(
        err instanceof Error && err.message ? err.message : `Failed to ${this.mode()} wardrobe item`,
      );
    } finally {
      this.working.set(false);
    }
  }
}
