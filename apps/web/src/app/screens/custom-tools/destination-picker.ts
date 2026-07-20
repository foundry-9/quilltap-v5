import { NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { CoreClient } from '../../core/core-client';
import type { CustomToolDestinations, DestinationStore } from '../../core/core-contract';
import * as api from './workbench.api';

/**
 * DestinationPicker — "Where shall Pascal keep this contrivance?" (v4
 * `components/custom-tools/DestinationPicker.tsx`, 217).
 *
 * Renders the §5.2 destination groups with a one-line consequence per tier —
 * this dialog doubles as the user's education about shadowing. A store already
 * carrying the same `name` is a BLOCKING warning (a same-store duplicate is a
 * load-time rejection) with a one-click "open the existing one instead"; the
 * same name in a DIFFERENT store is a non-blocking advisory.
 */

export interface PickedDestination {
  mountPointId: string;
  mountName: string;
}

/** Flatten every store the tree carries, for the duplicate-name survey. */
export function allStores(data: CustomToolDestinations): DestinationStore[] {
  return [
    ...(data.general ? [data.general] : []),
    ...data.projects.flatMap((p) => p.stores),
    ...data.groups.flatMap((g) => g.stores),
    ...data.characters.map((c) => ({
      mountPointId: c.mountPointId,
      mountName: c.mountName,
      existingToolNames: c.existingToolNames,
    })),
    ...data.other,
  ];
}

@Component({
  selector: 'qt-destination-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet],
  template: `
    <div class="qt-dialog-overlay" role="dialog" aria-modal="true">
      <div
        class="qt-card qt-shadow-lg rounded-lg border w-full max-w-lg max-h-[80vh] overflow-y-auto p-4 space-y-3"
      >
        <h2 class="qt-card-title text-base">Where shall Pascal keep this contrivance?</h2>

        @if (loading()) {
          <p class="text-sm qt-text-secondary">Surveying the premises…</p>
        }
        @if (error(); as message) {
          <p class="text-sm qt-text-destructive">{{ message }}</p>
        }

        @if (data(); as tree) {
          <div class="space-y-3">
            <div>
              <p class="text-sm font-medium">
                ◈ The General Store
                <span class="text-xs qt-text-secondary font-normal">— every chat, every character</span>
              </p>
              <div class="mt-1 space-y-0.5">
                @if (tree.general; as general) {
                  <ng-container
                    [ngTemplateOutlet]="row"
                    [ngTemplateOutletContext]="{ $implicit: general }"
                  />
                } @else {
                  <p class="text-xs qt-text-secondary px-3">
                    Not yet provisioned — the General store appears once first used.
                  </p>
                }
              </div>
            </div>

            @if (tree.projects.length > 0) {
              <div>
                <p class="text-sm font-medium">
                  ◈ Projects
                  <span class="text-xs qt-text-secondary font-normal">— chats in that project</span>
                </p>
                <div class="mt-1 space-y-0.5">
                  @for (project of tree.projects; track project.projectId) {
                    <div class="pl-2">
                      <p class="text-sm">▸ {{ project.projectName }}</p>
                      @for (store of project.stores; track store.mountPointId) {
                        <ng-container
                          [ngTemplateOutlet]="row"
                          [ngTemplateOutletContext]="{ $implicit: store }"
                        />
                      }
                    </div>
                  }
                </div>
              </div>
            }

            @if (tree.groups.length > 0) {
              <div>
                <p class="text-sm font-medium">
                  ◈ Groups
                  <span class="text-xs qt-text-secondary font-normal"
                    >— every member of the group</span
                  >
                </p>
                <div class="mt-1 space-y-0.5">
                  @for (group of tree.groups; track group.groupId) {
                    <div class="pl-2">
                      <p class="text-sm">▸ {{ group.groupName }}</p>
                      @for (store of group.stores; track store.mountPointId) {
                        <ng-container
                          [ngTemplateOutlet]="row"
                          [ngTemplateOutletContext]="{
                            $implicit: store,
                            label: store.official ? 'Official Store ★' : undefined,
                          }"
                        />
                      }
                    </div>
                  }
                </div>
              </div>
            }

            @if (tree.characters.length > 0) {
              <div>
                <p class="text-sm font-medium">
                  ◈ Character vaults
                  <span class="text-xs qt-text-secondary font-normal"
                    >— that character only — shadows every farther tier</span
                  >
                </p>
                <div class="mt-1 space-y-0.5">
                  @for (character of tree.characters; track character.mountPointId) {
                    <ng-container
                      [ngTemplateOutlet]="row"
                      [ngTemplateOutletContext]="{
                        $implicit: {
                          mountPointId: character.mountPointId,
                          mountName: character.characterName,
                          existingToolNames: character.existingToolNames,
                        },
                        label: 'their vault',
                      }"
                    />
                  }
                </div>
              </div>
            }

            @if (tree.other.length > 0) {
              <div>
                <p class="text-sm font-medium">
                  ◈ Other stores
                  <span class="text-xs qt-text-secondary font-normal"
                    >— not attached to anything — inert until linked</span
                  >
                </p>
                <div class="mt-1 space-y-0.5">
                  @for (store of tree.other; track store.mountPointId) {
                    <ng-container
                      [ngTemplateOutlet]="row"
                      [ngTemplateOutletContext]="{ $implicit: store }"
                    />
                  }
                </div>
              </div>
            }
          </div>
        }

        @if (duplicateHere() && selected(); as chosen) {
          <div class="text-xs qt-text-destructive space-y-1">
            <p>
              &ldquo;{{ toolName() }}&rdquo; is already on the table in {{ chosen.mountName }} — two
              files with one name in one store would both be refused at deal time.
            </p>
            <button
              type="button"
              class="qt-button qt-button-secondary qt-button-sm"
              (click)="openExisting.emit(chosen.mountPointId)"
            >
              Open the existing one instead
            </button>
          </div>
        }
        @if (!duplicateHere() && nameElsewhere()) {
          <p class="text-xs qt-text-secondary">
            This name is also on the table at another store; when both are in reach, the nearest
            tier wins.
          </p>
        }

        <div class="flex justify-end gap-2 pt-2">
          <button type="button" class="qt-button qt-button-ghost" (click)="cancel.emit()">
            Cancel
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="!selected() || duplicateHere()"
            (click)="pickSelected()"
          >
            Keep it here
          </button>
        </div>
      </div>
    </div>

    <ng-template #row let-store let-label="label">
      <button
        type="button"
        (click)="select(store)"
        class="block w-full text-left px-3 py-1.5 rounded text-sm"
        [class]="
          selected()?.mountPointId === store.mountPointId ? 'qt-bg-muted border' : 'hover:qt-bg-muted'
        "
      >
        <span class="flex items-center gap-2">
          <span class="qt-badge qt-badge-outline">{{ store.mountName }}</span>
          @if (label) {
            <span class="text-xs qt-text-secondary">{{ label }}</span>
          }
          @if (hasDuplicate(store)) {
            <span
              class="qt-badge qt-badge-destructive"
              [title]="'&quot;' + toolName() + '&quot; is already in this store'"
              >name taken</span
            >
          }
        </span>
      </button>
    </ng-template>
  `,
})
export class DestinationPicker {
  private readonly core = inject(CoreClient);

  /** The draft's tool name, for duplicate warnings. */
  readonly toolName = input.required<string>();

  readonly pick = output<PickedDestination>();
  readonly cancel = output<void>();
  /** Open an existing definition instead (same-store duplicate escape hatch). */
  readonly openExisting = output<string>();

  readonly selected = signal<PickedDestination | null>(null);
  readonly data = signal<CustomToolDestinations | null>(null);
  readonly loading = signal(true);
  readonly error = signal<string | null>(null);

  constructor() {
    void this.load();
  }

  private async load(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      this.data.set(await api.fetchDestinations(this.core));
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'The stores could not be listed.');
    } finally {
      this.loading.set(false);
    }
  }

  private readonly stores = computed(() => {
    const tree = this.data();
    return tree ? allStores(tree) : [];
  });

  /** The same name, in a store OTHER than the one selected — advisory only. */
  readonly nameElsewhere = computed(() => {
    const name = this.toolName();
    if (!this.data() || !name) return false;
    const chosen = this.selected();
    return this.stores().some(
      (s) => s.mountPointId !== chosen?.mountPointId && s.existingToolNames.includes(name),
    );
  });

  /**
   * The name already sits in the SELECTED store. This BLOCKS the save (v4
   * `:177` disables "Keep it here"): two files with one name in one store would
   * both be refused at deal time, so letting it through would produce a file
   * that can never load.
   */
  readonly duplicateHere = computed(() => {
    const name = this.toolName();
    const chosen = this.selected();
    if (!name || !chosen) return false;
    const store = this.stores().find((s) => s.mountPointId === chosen.mountPointId);
    return Boolean(store?.existingToolNames.includes(name));
  });

  protected hasDuplicate(store: DestinationStore): boolean {
    const name = this.toolName();
    return Boolean(name && store.existingToolNames.includes(name));
  }

  protected select(store: DestinationStore): void {
    this.selected.set({ mountPointId: store.mountPointId, mountName: store.mountName });
  }

  protected pickSelected(): void {
    const chosen = this.selected();
    if (chosen) this.pick.emit(chosen);
  }
}
