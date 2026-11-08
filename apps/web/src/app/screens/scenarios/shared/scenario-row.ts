import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { ScenarioDto } from '../../../core/core-contract';
import { ItemActionMenu, type MenuEntryLike } from '../../../item-browser/item-action-menu';

/**
 * ScenarioRow (v4 `components/scenarios/ScenarioRow.tsx`) — one width-adaptive
 * row inside {@link ScenariosManager}. Purely presentational; every mutation
 * lives in the manager (which owns the confirm/prompt/error flow).
 *
 * Container-query adaptive (Tailwind `@container`/`@lg`): a wide container shows
 * inline Edit / Rename / Archive / Delete buttons; a narrow one collapses the
 * four into a single `⋮` kebab — the shared `qt-item-action-menu` at the pin
 * `624d0d25c` (`29ba4d3f8` onto the fixed menu, then `600c2906f` onto
 * `ItemActionMenu`: fixed to the viewport, keyboard-operable, closed by an
 * outside press or a capture-phase Escape).
 */
@Component({
  selector: 'qt-scenario-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ItemActionMenu],
  template: `
    <li class="py-3 flex items-start gap-3">
      <input
        type="radio"
        class="qt-radio mt-1"
        [checked]="scenario().isDefault"
        (change)="setDefault.emit(scenario())"
        [disabled]="scenario().archived"
        [title]="radioTitle()"
        [attr.aria-label]="'Set ' + scenario().name + ' as ' + scopeLabel() + ' default'"
      />

      <div class="flex-1 min-w-0">
        <div class="flex items-baseline gap-2 flex-wrap">
          <h4 class="qt-label truncate">{{ scenario().name }}</h4>
          <span class="qt-text-xs qt-text-secondary truncate">{{ scenario().filename }}.md</span>
          @if (scenario().isDefault) {
            <span class="qt-badge qt-badge-primary">Default</span>
          }
          @if (scenario().archived) {
            <span class="qt-badge qt-badge-secondary">Archived</span>
          }
        </div>
        @if (scenario().description) {
          <p class="qt-text-small qt-text-secondary mt-1">{{ scenario().description }}</p>
        }
      </div>

      <!-- Wide container: inline action buttons. -->
      <div class="hidden @lg:flex items-center gap-1 shrink-0">
        <button
          type="button"
          class="qt-button qt-button-ghost qt-button-sm"
          title="Edit scenario"
          (click)="edit.emit(scenario())"
        >
          Edit
        </button>
        <button
          type="button"
          class="qt-button qt-button-ghost qt-button-sm"
          title="Rename file"
          (click)="rename.emit(scenario())"
        >
          Rename
        </button>
        <button
          type="button"
          class="qt-button qt-button-ghost qt-button-sm"
          [title]="archiveTitle()"
          (click)="toggleArchived.emit(scenario())"
        >
          {{ archiveLabel() }}
        </button>
        <button
          type="button"
          class="qt-button qt-button-ghost qt-button-sm qt-text-destructive"
          title="Delete scenario"
          (click)="delete.emit(scenario())"
        >
          Delete
        </button>
      </div>

      <!-- Narrow container: kebab menu holding the same four actions — the
           shared item-browser menu (v4 600c2906f, pin :128-141). -->
      <div class="@lg:hidden shrink-0">
        <qt-item-action-menu
          [label]="'More actions for ' + scenario().name"
          [room]="200"
          minWidthClass="min-w-[10rem]"
          [entries]="kebabEntries()"
        />
      </div>
    </li>
  `,
})
export class ScenarioRow {
  readonly scenario = input.required<ScenarioDto>();
  /** Used in the default radio's title/aria copy. e.g. "project" or "general". */
  readonly scopeLabel = input<string>('default');
  readonly setDefault = output<ScenarioDto>();
  readonly edit = output<ScenarioDto>();
  readonly rename = output<ScenarioDto>();
  readonly delete = output<ScenarioDto>();
  /** Archive an active scenario, or restore an archived one. */
  readonly toggleArchived = output<ScenarioDto>();

  /** v4 `archiveLabel` (`:52`). */
  protected readonly archiveLabel = computed(() =>
    this.scenario().archived ? 'Restore' : 'Archive',
  );

  protected readonly archiveTitle = computed(() =>
    this.scenario().archived
      ? 'Restore this scenario to the pickers'
      : 'Hide this scenario from the pickers',
  );

  /**
   * v4 `:87-94`. An archived scenario is barred from default resolution
   * server-side, so the radio is disabled and says why — offering it would be a
   * control that quietly does nothing.
   */
  protected readonly radioTitle = computed(() => {
    const s = this.scenario();
    if (s.archived) return 'Archived scenarios cannot be the default';
    return s.isDefault ? `${this.scopeLabel()} default` : `Set as ${this.scopeLabel()} default`;
  });

  /** v4 pin `:134-139`. */
  protected readonly kebabEntries = computed<readonly MenuEntryLike[]>(() => {
    const scenario = this.scenario();
    return [
      { key: 'edit', label: 'Edit', onSelect: () => this.edit.emit(scenario) },
      { key: 'rename', label: 'Rename', onSelect: () => this.rename.emit(scenario) },
      { key: 'archive', label: this.archiveLabel(), onSelect: () => this.toggleArchived.emit(scenario) },
      { key: 'delete', label: 'Delete', destructive: true, onSelect: () => this.delete.emit(scenario) },
    ];
  });
}
