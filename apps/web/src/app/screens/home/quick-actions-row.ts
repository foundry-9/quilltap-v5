import { ChangeDetectionStrategy, Component, input, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQueryClient } from '@tanstack/angular-query-experimental';

import { ProjectCreateDialog } from '../prospero/project-create-dialog';
import { Icon } from '../../ui/icon';
import { homeKeys } from './home.api';

/**
 * The quick-action buttons row (v4 `components/homepage/QuickActionsRow.tsx`).
 * v4's five actions, ported as four: Start a Chat, Start Autonomous Room,
 * Continue Last (disabled when there is no `lastChatId`), and New Project
 * (reuses the Prospero {@link ProjectCreateDialog}). v4's fifth action,
 * **Generate Image** (`/generate-image`, `GenerateImageView`), is OMITTED —
 * that screen is unported (the P4.6av tier-3 deferral, banked for the M6
 * screen-parity pool); no dead links.
 */
@Component({
  selector: 'qt-quick-actions-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon, ProjectCreateDialog],
  template: `
    <div class="qt-quick-actions mb-8">
      <!-- Start a Chat -->
      <a routerLink="/salon/new" class="qt-button qt-button-primary gap-2">
        <qt-icon name="chat" class="w-4 h-4" />
        <span class="hidden sm:inline">Start a Chat</span>
        <span class="sm:hidden">Chat</span>
      </a>

      <!-- Start an Autonomous Room -->
      <a
        routerLink="/salon/new"
        [queryParams]="{ autonomous: '1' }"
        class="qt-button qt-button-secondary gap-2"
        title="Create a character-to-character room that runs on a schedule or on demand"
      >
        <qt-icon name="clock" class="w-4 h-4" />
        <span class="hidden sm:inline">Start Autonomous Room</span>
        <span class="sm:hidden">Auto Room</span>
      </a>

      <!-- Continue Last Chat -->
      @if (lastChatId(); as id) {
        <a [routerLink]="['/salon', id]" class="qt-button qt-button-secondary gap-2">
          <qt-icon name="play" class="w-4 h-4" />
          <span class="hidden sm:inline">Continue Last</span>
          <span class="sm:hidden">Continue</span>
        </a>
      } @else {
        <button
          disabled
          class="qt-button qt-button-secondary gap-2 opacity-50 cursor-not-allowed"
          title="No recent chats"
        >
          <qt-icon name="play" class="w-4 h-4" />
          <span class="hidden sm:inline">Continue Last</span>
          <span class="sm:hidden">Continue</span>
        </button>
      }

      <!-- New Project -->
      <button
        type="button"
        class="qt-button qt-button-secondary gap-2"
        (click)="projectDialogOpen.set(true)"
      >
        <qt-icon name="folder-plus" class="w-4 h-4" />
        <span class="hidden sm:inline">New Project</span>
        <span class="sm:hidden">Project</span>
      </button>
    </div>

    <!-- Create Project Dialog -->
    @if (projectDialogOpen()) {
      <qt-project-create-dialog
        (close)="projectDialogOpen.set(false)"
        (created)="onProjectCreated()"
      />
    }
  `,
})
export class QuickActionsRow {
  readonly lastChatId = input.required<string | null>();

  private readonly queryClient = injectQueryClient();
  protected readonly projectDialogOpen = signal(false);

  /**
   * v4 `handleProjectCreate` closes the dialog and stays put ("Optionally
   * refresh or navigate" — it does neither; the server-rendered `/` refetches
   * on the next visit). The SPA equivalent of that next-visit freshness is
   * invalidating the home query so the Projects section catches up.
   */
  protected onProjectCreated(): void {
    this.projectDialogOpen.set(false);
    void this.queryClient.invalidateQueries({ queryKey: homeKeys.all });
  }
}
