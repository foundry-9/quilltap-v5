import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { Router } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { WORKSPACE_TAB_ID } from '../../workspace/workspace-contract';
import { CoreClient } from '../../core/core-client';
import { GroupCard } from './group-card';
import { GroupCreateDialog } from './group-create-dialog';
import { deleteGroup, fetchGroups, groupKeys, type GroupCardModel } from './groups.api';
import { ToastService } from '../../ui/toast.service';

/**
 * The Groups section at the top of the Characters page (v4 `AuroraView.tsx`
 * groups block + `GroupsGrid`). A `Create Group` button, a 1/2/3-col card grid
 * over the `groupList` dispatch, an empty state, and immediate (no-confirm)
 * delete. Card click / post-create navigate into the routed editor
 * (`/characters/groups/:id` — v5's path idiom; v4 used `/aurora/groups/:id`).
 */
@Component({
  selector: 'qt-groups-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [GroupCard, GroupCreateDialog],
  template: `
    <div class="mt-8">
      <h2 class="qt-heading-2 mb-6 text-foreground">Groups</h2>

      @if (groups().length === 0) {
        <div
          class="rounded-2xl border border-dashed qt-border-default/70 qt-bg-card/80 px-8 py-12 text-center qt-shadow-sm"
        >
          <p class="mb-4 text-lg qt-text-secondary">No groups yet</p>
          <button
            type="button"
            class="qt-text-primary hover:text-primary/80"
            (click)="createOpen.set(true)"
          >
            Create your first group
          </button>
        </div>
      } @else {
        <div class="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
          @for (group of groups(); track group.id) {
            <qt-group-card
              [group]="group"
              [inTab]="tabId != null"
              (open)="onOpenGroup($event)"
              (delete)="onDelete($event)"
            />
          }
        </div>
      }
    </div>

    @if (createOpen()) {
      <qt-group-create-dialog (close)="createOpen.set(false)" (created)="onCreated($event)" />
    }
  `,
})
export class GroupsSection {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();
  private readonly router = inject(Router);
  /** Non-null ⇒ hosted; a group opens via the `openGroup` output for in-tab drill. */
  protected readonly tabId = inject(WORKSPACE_TAB_ID, { optional: true });

  /** Emitted (instead of routing) when hosted, so the aurora list drills in place. */
  readonly openGroup = output<string>();

  protected readonly createOpen = signal(false);

  protected readonly groupsQuery = injectQuery(() => ({
    queryKey: groupKeys.list(),
    queryFn: (): Promise<GroupCardModel[]> => fetchGroups(this.core),
  }));

  protected readonly groups = computed(() => this.groupsQuery.data() ?? []);

  /** Open the Create Group dialog — called by the Characters-page toolbar button. */
  openCreate(): void {
    this.createOpen.set(true);
  }

  protected onOpenGroup(id: string): void {
    if (this.tabId != null) {
      this.openGroup.emit(id);
      return;
    }
    void this.router.navigate(['/characters/groups', id]);
  }

  protected onCreated(id: string): void {
    this.createOpen.set(false);
    void this.queryClient.invalidateQueries({ queryKey: groupKeys.list() });
    if (this.tabId != null) {
      this.openGroup.emit(id);
      return;
    }
    void this.router.navigate(['/characters/groups', id]);
  }

  protected async onDelete(id: string): Promise<void> {
    // v4: immediate delete, NO confirm dialog. Optimistically drop the card.
    const key = groupKeys.list();
    const previous = this.queryClient.getQueryData<GroupCardModel[]>(key);
    this.queryClient.setQueryData<GroupCardModel[]>(key, (prev) =>
      (prev ?? []).filter((g) => g.id !== id),
    );
    try {
      await deleteGroup(this.core, id);
      this.toasts.showSuccess('Group deleted successfully!');
    } catch (err) {
      this.queryClient.setQueryData<GroupCardModel[]>(key, previous);
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to delete group');
    } finally {
      await this.queryClient.invalidateQueries({ queryKey: key });
    }
  }
}
