import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { RouterLink } from '@angular/router';

import { Icon } from '../../ui/icon';
import { formatMessageTime } from './format-time';
import type { HomepageProject } from './home.api';

/**
 * One project row in the homepage Active Projects list (v4
 * `components/homepage/ProjectItem.tsx`): the folder chip tinted with the
 * project color, name + optional description, relative last-activity + chat
 * count, and the trailing new-chat-in-project action (the same
 * `/salon/new?projectId=` navigation the Prospero project header uses).
 */
@Component({
  selector: 'qt-project-item',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon],
  template: `
    <div class="flex items-center gap-3 p-2 rounded-lg hover:qt-bg-muted/50 transition-colors">
      <a
        [routerLink]="['/prospero', project().id]"
        class="flex items-center gap-3 flex-1 min-w-0"
      >
        <div class="shrink-0 w-8 h-8 flex items-center justify-center rounded-md qt-bg-muted">
          <qt-icon name="folder" class="w-5 h-5" [style.color]="project().color || null" />
        </div>
        <div class="flex-1 min-w-0">
          <p class="qt-card-title truncate">{{ project().name }}</p>
          @if (project().description) {
            <p class="qt-card-subtitle truncate">{{ project().description }}</p>
          }
        </div>
        <div class="flex flex-col items-end shrink-0">
          <span class="qt-meta">{{ activityLabel() }}</span>
          <span class="qt-meta text-primary">{{ chatCountLabel() }}</span>
        </div>
      </a>
      <a
        routerLink="/salon/new"
        [queryParams]="{ projectId: project().id }"
        class="shrink-0 qt-button-success qt-button-sm !px-2"
        [title]="'Start a new chat in ' + project().name"
        [attr.aria-label]="'Start a new chat in ' + project().name"
      >
        <qt-icon name="chat" class="w-4 h-4" />
      </a>
    </div>
  `,
})
export class ProjectItem {
  readonly project = input.required<HomepageProject>();

  protected readonly activityLabel = computed(() =>
    formatMessageTime(this.project().lastActivity),
  );

  protected readonly chatCountLabel = computed(() => {
    const n = this.project().chatCount;
    return `${n} ${n === 1 ? 'chat' : 'chats'}`;
  });
}
