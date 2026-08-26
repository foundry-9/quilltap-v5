import {
  ChangeDetectionStrategy,
  Component,
  computed,
  ElementRef,
  OnDestroy,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import type { ScenarioDto } from '../../../core/core-contract';

/**
 * ScenarioRow (v4 `components/scenarios/ScenarioRow.tsx`) — one width-adaptive
 * row inside {@link ScenariosManager}. Purely presentational; every mutation
 * lives in the manager (which owns the confirm/prompt/error flow).
 *
 * Container-query adaptive (Tailwind `@container`/`@lg`): a wide container shows
 * inline Edit / Rename / Archive / Delete buttons; a narrow one collapses the
 * four into a single `⋮` kebab (closes on outside mousedown + capture-phase
 * Escape, mirroring the house wardrobe-row pattern).
 */
@Component({
  selector: 'qt-scenario-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
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

      <!-- Narrow container: kebab menu holding the same four actions. -->
      <div class="relative @lg:hidden shrink-0">
        <button
          type="button"
          class="qt-button-ghost qt-button-sm"
          [attr.aria-label]="'More actions for ' + scenario().name"
          title="More actions"
          aria-haspopup="menu"
          [attr.aria-expanded]="kebabOpen()"
          (click)="toggleKebab()"
        >
          ⋮
        </button>
        @if (kebabOpen()) {
          <div role="menu" class="qt-dropdown absolute right-0 top-full mt-1">
            <button
              type="button"
              role="menuitem"
              class="qt-dropdown-item w-full text-left"
              (click)="pick(edit)"
            >
              Edit
            </button>
            <button
              type="button"
              role="menuitem"
              class="qt-dropdown-item w-full text-left"
              (click)="pick(rename)"
            >
              Rename
            </button>
            <button
              type="button"
              role="menuitem"
              class="qt-dropdown-item w-full text-left"
              (click)="pick(toggleArchived)"
            >
              {{ archiveLabel() }}
            </button>
            <button
              type="button"
              role="menuitem"
              class="qt-dropdown-item w-full text-left qt-text-destructive"
              (click)="pick(delete)"
            >
              Delete
            </button>
          </div>
        }
      </div>
    </li>
  `,
})
export class ScenarioRow implements OnDestroy {
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

  private readonly host = inject(ElementRef<HTMLElement>);
  protected readonly kebabOpen = signal(false);

  private readonly onDocMouseDown = (e: MouseEvent): void => {
    if (!this.host.nativeElement.contains(e.target as Node)) {
      this.kebabOpen.set(false);
    }
  };

  private readonly onKeyDown = (e: KeyboardEvent): void => {
    if (e.key !== 'Escape') {
      return;
    }
    e.stopPropagation();
    e.preventDefault();
    this.kebabOpen.set(false);
  };

  protected toggleKebab(): void {
    this.kebabOpen.update((v) => !v);
    if (this.kebabOpen()) {
      document.addEventListener('mousedown', this.onDocMouseDown);
      document.addEventListener('keydown', this.onKeyDown, true);
    } else {
      this.detach();
    }
  }

  protected pick(out: { emit(v: ScenarioDto): void }): void {
    this.kebabOpen.set(false);
    this.detach();
    out.emit(this.scenario());
  }

  ngOnDestroy(): void {
    this.detach();
  }

  private detach(): void {
    document.removeEventListener('mousedown', this.onDocMouseDown);
    document.removeEventListener('keydown', this.onKeyDown, true);
  }
}
