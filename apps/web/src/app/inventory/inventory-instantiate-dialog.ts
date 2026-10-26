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
import type { ResolvedInventoryItem } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import { decodeInventoryContainer, encodeInventoryContainer, type InventoryContainer } from './inventory-container';
import { InventoryPicture } from './inventory-glyph';
import type { InventoryItemActions } from './inventory-item-actions';
import { describeInventoryRefusal } from './inventory-refusal';
import { catalogueGroups, type InventoryCatalogue } from './inventory-transfer-dialog';
import { instantiateInventoryItem, listInventoryArchetypes, type InventoryInstantiateBody } from './inventory.api';

/**
 * Make an instance of an archetype — a port of v4
 * `components/inventory/InventoryInstantiateDialog.tsx` (`322a5488e`). Two
 * ways in: from a tier ("+ From archetype" — choose one of the archetypes the
 * tier can reach, shown with their pictures), or from an archetype's row
 * ("Instantiate…" — choose the tier the instance lands in). Instantiating is
 * not copying: `canCopy` does not apply.
 *
 * An archetype in a project store reaches a character's vault through that
 * project, so a row-opened dialog names the archetype's project when the host
 * names none (`effectiveProjectId`). The body omits `folder` and `projectId`
 * unless set, as v4's `|| undefined` / `?? undefined` drop them from the JSON.
 */
@Component({
  selector: 'qt-inventory-instantiate-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, InventoryPicture],
  host: { '(document:keydown.escape)': 'onEscape()' },
  template: `
    <qt-modal
      [title]="title()"
      maxWidth="lg"
      overlayClass="z-[70]"
      [closeOnBackdrop]="!working()"
      (close)="closed.emit()"
    >
      <p class="qt-text-xs qt-text-secondary mb-2">
        The instance follows its archetype — later edits reach it — except for whatever you override on it.
      </p>
      @if (archetype()) {
        <label class="text-sm text-foreground" for="inventory-instantiate-target">Into</label>
        <select
          id="inventory-instantiate-target"
          class="qt-select w-full mt-1"
          [value]="target()"
          (change)="target.set($any($event.target).value)"
        >
          <option value="" disabled>Choose an inventory</option>
          @for (group of groups(); track group.label) {
            <optgroup [label]="group.label">
              @for (option of group.options; track option.value) {
                <option [value]="option.value">{{ option.name }}</option>
              }
            </optgroup>
          }
        </select>
        <p class="qt-text-xs qt-text-secondary mt-1">
          The destination must be able to reach the archetype (a character reaches their own vault, their groups, a
          named project and General).
        </p>
      } @else if (loading()) {
        <div class="text-sm qt-text-secondary">Looking for archetypes…</div>
      } @else if (archetypes().length === 0) {
        <div class="text-sm qt-text-secondary">No archetypes are within reach of this inventory yet.</div>
      } @else {
        <ul role="listbox" aria-label="Archetypes" class="space-y-1 max-h-[50vh] overflow-y-auto">
          @for (a of archetypes(); track a.id) {
            <li>
              <button
                type="button"
                role="option"
                [attr.aria-selected]="chosen() === a.id"
                (click)="chosen.set(a.id)"
                [attr.class]="'w-full flex items-center gap-2 px-2 py-1 rounded text-left ' + (chosen() === a.id ? 'qt-bg-muted' : '')"
              >
                <qt-inventory-picture [item]="a" />
                <span class="flex-1 min-w-0">
                  <span class="block text-sm text-foreground truncate">{{ a.title || a.name }}</span>
                  @if (a.description) {
                    <span class="block qt-text-xs qt-text-secondary truncate">{{ a.description }}</span>
                  }
                </span>
              </button>
            </li>
          }
        </ul>
      }

      <div qt-modal-footer class="flex justify-end gap-2 w-full">
        <button type="button" class="qt-button-secondary qt-button-sm" (click)="closed.emit()" [disabled]="working()">
          Cancel
        </button>
        <button
          type="button"
          class="qt-button-primary qt-button-sm"
          (click)="submit()"
          [disabled]="!chosen() || !target() || working()"
        >
          {{ working() ? 'Making…' : 'Instantiate' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class InventoryInstantiateDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  /** Choose an archetype for this destination… */
  readonly destination = input<{ container: InventoryContainer; folder: string } | null>(null);
  /** …or a destination for this archetype. */
  readonly archetype = input<ResolvedInventoryItem | null>(null);
  /** The project tier a character's archetypes may come from. */
  readonly projectId = input<string | null>(null);
  readonly catalogue = input.required<InventoryCatalogue>();
  readonly actions = input.required<InventoryItemActions>();
  readonly closed = output<void>();

  protected readonly chosen = signal<string | null>(null);
  protected readonly target = signal('');
  protected readonly working = signal(false);
  protected readonly archetypes = signal<ResolvedInventoryItem[]>([]);
  protected readonly loading = signal(false);

  /** v4 `:57-58`. */
  private readonly effectiveProjectId = computed(() => {
    const archetype = this.archetype();
    return this.projectId() ?? (archetype?.origin?.scope === 'project' ? archetype.origin.id : null);
  });
  protected readonly title = computed(() => {
    const archetype = this.archetype();
    return archetype ? `Instantiate "${archetype.title || archetype.name}"` : 'Make an object from an archetype';
  });
  protected readonly groups = computed(() => catalogueGroups(this.catalogue()));

  constructor() {
    // v4's `useState` initialisers, from the inputs the dialog mounts with.
    effect(() => {
      const archetype = this.archetype();
      const destination = this.destination();
      untracked(() => {
        this.chosen.set(archetype?.id ?? null);
        this.target.set(destination ? encodeInventoryContainer(destination.container) : '');
      });
    });
    // v4 `useInventoryArchetypes(destination?.container ?? null, effectiveProjectId)`.
    effect(() => {
      const container = this.destination()?.container ?? null;
      const projectId = this.effectiveProjectId();
      untracked(() => {
        if (!container) {
          this.archetypes.set([]);
          this.loading.set(false);
          return;
        }
        this.loading.set(true);
        void listInventoryArchetypes(this.core, container, projectId)
          .then((items) => this.archetypes.set(items), () => this.archetypes.set([]))
          .finally(() => this.loading.set(false));
      });
    });
  }

  /** v4 `submit` (`:64-82`). */
  protected async submit(): Promise<void> {
    const to = decodeInventoryContainer(this.target());
    const archetypeId = this.chosen();
    if (!to || !archetypeId) return;
    const folder = this.destination()?.folder || undefined;
    const projectId = this.effectiveProjectId() ?? undefined;
    const body: InventoryInstantiateBody = {
      archetypeId,
      ...(folder !== undefined ? { folder } : {}),
      ...(projectId !== undefined ? { projectId } : {}),
    };
    this.working.set(true);
    try {
      const made = await instantiateInventoryItem(this.core, to, body);
      this.toasts.showSuccess(`"${made.title || made.name}" is made from its archetype`);
      this.closed.emit();
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, 'The archetype would not be instantiated'));
    } finally {
      this.working.set(false);
      void this.actions().refresh();
    }
  }

  protected onEscape(): void {
    if (!this.working()) this.closed.emit();
  }
}
