import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import { Icon } from '../../ui/icon';
import type { GroupCardModel } from './groups.api';

/**
 * One group card (v4 `app/aurora/components/GroupCard.tsx`): a color swatch +
 * emoji (users-icon fallback), name, member count, 2-line-clamped description,
 * an **Edit** link into the editor and a **Delete** trash button.
 *
 * v4 delete is IMMEDIATE with NO confirm dialog — ported faithfully. The whole
 * card is clickable (routes to the editor) except the action buttons.
 */
@Component({
  selector: 'qt-group-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon],
  template: `
    <div
      class="qt-entity-card cursor-pointer hover:qt-border-primary/50 transition-colors"
      (click)="onCardClick($event)"
    >
      <div class="flex items-start justify-between mb-4">
        <div class="flex items-center gap-3">
          <div
            class="w-10 h-10 rounded-lg flex items-center justify-center text-lg"
            [style.backgroundColor]="group().color || 'var(--muted)'"
          >
            @if (group().icon) {
              {{ group().icon }}
            } @else {
              <qt-icon name="users" class="w-5 h-5 qt-text-secondary" />
            }
          </div>
          <div>
            <h2 class="text-xl font-semibold text-foreground">{{ group().name }}</h2>
            <p class="qt-text-small">{{ memberLabel() }}</p>
          </div>
        </div>
      </div>

      @if (group().description) {
        <p class="line-clamp-2 qt-text-small mb-4">{{ group().description }}</p>
      }

      <div class="qt-entity-card-actions flex gap-2">
        @if (inTab()) {
          <button
            type="button"
            class="inline-flex flex-1 items-center justify-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground qt-shadow-sm transition hover:qt-bg-primary/90"
            title="Edit group"
            (click)="$event.stopPropagation(); open.emit(group().id)"
          >
            Edit
          </button>
        } @else {
          <a
            [routerLink]="['/characters/groups', group().id]"
            class="inline-flex flex-1 items-center justify-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground qt-shadow-sm transition hover:qt-bg-primary/90"
            title="Edit group"
            (click)="$event.stopPropagation()"
          >
            Edit
          </a>
        }
        <button
          type="button"
          class="qt-button-destructive qt-shadow-sm"
          title="Delete group"
          aria-label="Delete group"
          (click)="onDelete($event)"
        >
          <qt-icon name="trash" class="w-5 h-5" />
        </button>
      </div>
    </div>
  `,
})
export class GroupCard {
  readonly group = input.required<GroupCardModel>();
  /** Hosted as a workspace tab ⇒ the Edit affordance drills in place (emits). */
  readonly inTab = input<boolean>(false);
  readonly open = output<string>();
  readonly delete = output<string>();

  protected readonly memberLabel = computed(() => {
    const n = this.group().memberCount ?? 0;
    return `${n} member${n !== 1 ? 's' : ''}`;
  });

  protected onCardClick(event: MouseEvent): void {
    // v4 ignores clicks that land on a button or link (their own handlers fire).
    const target = event.target as HTMLElement;
    if (target.closest('button') || target.closest('a')) {
      return;
    }
    this.open.emit(this.group().id);
  }

  protected onDelete(event: Event): void {
    event.stopPropagation();
    this.delete.emit(this.group().id);
  }
}
