import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import { Icon } from '../../ui/icon';
import type { ProjectCardModel } from './projects.api';

/**
 * One project card (v4 `app/prospero/components/ProjectCard.tsx`): a color swatch
 * + emoji (folder fallback), name, "N chats • M files", 2-line-clamp description,
 * an **Open** link into the detail and a **Delete** trash button (which opens the
 * confirm dialog). The whole card is clickable except the action buttons.
 */
@Component({
  selector: 'qt-project-card',
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
            [style.backgroundColor]="project().color || 'var(--muted)'"
          >
            @if (project().icon) {
              {{ project().icon }}
            } @else {
              <qt-icon name="folder" class="w-5 h-5 qt-text-secondary" />
            }
          </div>
          <div>
            <h2 class="text-xl font-semibold text-foreground">{{ project().name }}</h2>
            <p class="qt-text-small">{{ countLabel() }}</p>
          </div>
        </div>
      </div>

      @if (project().description) {
        <p class="line-clamp-2 qt-text-small mb-4">{{ project().description }}</p>
      }

      <div class="qt-entity-card-actions flex gap-2">
        @if (inTab()) {
          <!-- Hosted as a workspace tab: drilling is state, not navigation. -->
          <button
            type="button"
            class="inline-flex flex-1 items-center justify-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground qt-shadow-sm transition hover:qt-bg-primary/90"
            (click)="$event.stopPropagation(); open.emit(project().id)"
          >
            Open
          </button>
        } @else {
          <a
            [routerLink]="['/prospero', project().id]"
            class="inline-flex flex-1 items-center justify-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground qt-shadow-sm transition hover:qt-bg-primary/90"
            (click)="$event.stopPropagation()"
          >
            Open
          </a>
        }
        <button
          type="button"
          class="qt-button-destructive qt-shadow-sm"
          title="Delete project"
          aria-label="Delete project"
          (click)="onDelete($event)"
        >
          <qt-icon name="trash" class="w-5 h-5" />
        </button>
      </div>
    </div>
  `,
})
export class ProjectCard {
  readonly project = input.required<ProjectCardModel>();
  /** Hosted as a workspace tab ⇒ the Open affordance drills in place (emits). */
  readonly inTab = input<boolean>(false);
  readonly open = output<string>();
  readonly delete = output<string>();

  protected readonly countLabel = computed(() => {
    const c = this.project().chatCount ?? 0;
    const f = this.project().fileCount ?? 0;
    return `${c} chat${c !== 1 ? 's' : ''} • ${f} file${f !== 1 ? 's' : ''}`;
  });

  protected onCardClick(event: MouseEvent): void {
    const target = event.target as HTMLElement;
    if (target.closest('button') || target.closest('a')) {
      return;
    }
    this.open.emit(this.project().id);
  }

  protected onDelete(event: Event): void {
    event.stopPropagation();
    this.delete.emit(this.project().id);
  }
}
