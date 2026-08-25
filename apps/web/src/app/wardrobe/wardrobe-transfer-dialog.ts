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
  type WardrobeTransferComponentMode,
  type WardrobeTransferScope,
} from './wardrobe.api';
import { sameWardrobeContainer, type WardrobeContainer } from './wardrobe-container';
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

              @if (visibleDestinations()?.general?.available) {
                <optgroup label="General">
                  <option
                    [value]="generalValue"
                    [selected]="selectedDestination() === generalValue"
                  >
                    {{ visibleDestinations()?.general?.label }}
                  </option>
                </optgroup>
              }

              @if ((visibleDestinations()?.projects ?? []).length > 0) {
                <optgroup label="Projects">
                  @for (project of visibleDestinations()?.projects ?? []; track project.id) {
                    <option
                      [value]="encode('project', project.id)"
                      [selected]="selectedDestination() === encode('project', project.id)"
                    >
                      {{ project.name }}
                    </option>
                  }
                </optgroup>
              }

              @if ((visibleDestinations()?.groups ?? []).length > 0) {
                <optgroup label="Groups">
                  @for (group of visibleDestinations()?.groups ?? []; track group.id) {
                    <option
                      [value]="encode('group', group.id)"
                      [selected]="selectedDestination() === encode('group', group.id)"
                    >
                      {{ group.name }}
                    </option>
                  }
                </optgroup>
              }

              @if ((visibleDestinations()?.users ?? []).length > 0) {
                <optgroup label="Users">
                  @for (user of visibleDestinations()?.users ?? []; track user.id) {
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

        @if (isComposite()) {
          <fieldset>
            <legend class="qt-text-sm qt-text-secondary mb-1">{{ componentCountLabel() }}</legend>
            <div class="space-y-1">
              @for (opt of componentOptions(); track opt.value) {
                <label class="flex items-center gap-2 cursor-pointer qt-text-sm">
                  <input
                    type="radio"
                    name="wardrobe-transfer-components"
                    class="qt-radio"
                    [checked]="componentMode() === opt.value"
                    (change)="componentMode.set(opt.value)"
                    [disabled]="working()"
                  />
                  <span>{{ opt.label }}</span>
                </label>
              }
            </div>
            <p class="qt-text-xs qt-text-secondary mt-1">
              All or nothing — the choice covers every component, nested pieces included. Only
              components living in the same wardrobe travel; pieces from a shared tier stay put.
              The transferred outfit is rewired to point at whichever components arrive with it.
            </p>
          </fieldset>
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

  protected readonly generalValue = encodeDestination('general', null);

  protected readonly loadingDestinations = signal(false);
  protected readonly destinations = signal<TransferDestinationsPayload | null>(null);
  protected readonly selectedDestination = signal('');
  protected readonly working = signal(false);
  /**
   * Composite outfits prompt for their components — all or nothing. A move
   * defaults to moving them along; a copy defaults to copying them, since an
   * outfit that arrives without its pieces is rarely what anyone meant
   * (v4 `:88-94`). The component mounts per open, so v4's reset-on-open
   * collapses into this initial read.
   */
  protected readonly isComposite = computed(() => (this.item().componentItemIds ?? []).length > 0);
  protected readonly componentMode = signal<WardrobeTransferComponentMode>('copy');

  private loaded = false;

  constructor() {
    // Load destinations once on mount (v4 :66-90 — the component mounts per
    // open, so the reset-on-open collapses into the initial state here).
    effect(() => {
      if (this.loaded) return;
      this.loaded = true;
      untracked(() => {
        this.componentMode.set(this.mode() === 'move' ? 'move' : 'copy');
        void this.loadDestinations();
      });
    });
  }

  private async loadDestinations(): Promise<void> {
    this.loadingDestinations.set(true);
    try {
      const data = await dispatchWardrobe(this.core, { type: 'wardrobeTransferDestinations' });
      const destinations = data['destinations'] as TransferDestinationsPayload;
      this.destinations.set(destinations);
      // v4 `:101-108` — preselect General when available AND not the item's own
      // home; preselecting a destination the list then hides would arm the
      // submit button against nothing.
      const generalExcluded = sameWardrobeContainer(this.excludeDestination(), {
        scope: 'general',
        id: null,
      });
      if (destinations.general.available && !generalExcluded) {
        this.selectedDestination.set(this.generalValue);
      }
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : String(err));
    } finally {
      this.loadingDestinations.set(false);
    }
  }

  /**
   * The item's known home container is dropped from the list — the server
   * refuses same-place transfers, so offering it would only invite a scolding
   * (v4 `:126-148`).
   */
  protected readonly visibleDestinations = computed<TransferDestinationsPayload | null>(() => {
    const destinations = this.destinations();
    if (!destinations) return null;
    const exclude = this.excludeDestination();
    if (!exclude) return destinations;
    const excluded = (scope: WardrobeTransferScope, id: string | null): boolean =>
      sameWardrobeContainer(exclude, { scope, id });
    return {
      general: {
        ...destinations.general,
        available: destinations.general.available && !excluded('general', null),
      },
      projects: destinations.projects.filter((p) => !excluded('project', p.id)),
      groups: destinations.groups.filter((g) => !excluded('group', g.id)),
      users: destinations.users.filter((u) => !excluded('character', u.id)),
    };
  });

  protected readonly selection = computed(() => decodeDestination(this.selectedDestination()));

  /**
   * v4 `:290-327` — the radio pair. Moving offers move / copy / leave; copying
   * offers copy / don't. Copy + `components: 'move'` is unreachable from here
   * by construction, and the server refuses it besides.
   */
  protected readonly componentOptions = computed<
    Array<{ value: WardrobeTransferComponentMode; label: string }>
  >(() =>
    this.mode() === 'move'
      ? [
          { value: 'move', label: 'Move the components along with it' },
          { value: 'copy', label: 'Copy the components (originals stay behind)' },
          { value: 'none', label: 'Leave the components behind' },
        ]
      : [
          { value: 'copy', label: 'Copy the components along with it' },
          { value: 'none', label: 'Copy the outfit alone' },
        ],
  );

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

  /** v4 `:181-185` — the all-empty placeholder option. */
  protected readonly noDestinations = computed(() => {
    const d = this.visibleDestinations();
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
      this.toasts.showError(err instanceof Error ? err.message : String(err));
    } finally {
      this.working.set(false);
    }
  }
}
