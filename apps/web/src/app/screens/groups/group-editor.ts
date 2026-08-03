import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { map } from 'rxjs';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { GroupMemberSummary, GroupSummary } from '../../core/core-contract';
import { StateEditorModal } from '../../shared/state/state-editor-modal';
import { Icon } from '../../ui/icon';
import { LoadingState } from '../../ui/loading-state';
import { ToastService } from '../../ui/toast.service';
import { fetchCharacterList } from '../characters/characters.api';
import { GroupMembersCard } from './group-members-card';
import { GroupStoresCard } from './group-stores-card';
import {
  addGroupMember,
  fetchGroup,
  fetchGroupMembers,
  fetchGroupStores,
  groupKeys,
  removeGroupMember,
  unlinkGroupStore,
  updateGroup,
} from './groups.api';

/**
 * The routed group editor (v4 `app/aurora/groups/[id]/GroupDetailView.tsx`).
 * Route: `/characters/groups/:id` (v5's path idiom — v4 used `/aurora/groups/[id]`;
 * recorded divergence). An explicit-Save `<form>` (name*, description, color,
 * icon — NO autosave) over two collapsed-by-default cards (Members, Scriptorium).
 * Groups have NO image upload — colour + emoji only.
 */
@Component({
  selector: 'qt-group-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, Icon, LoadingState, GroupMembersCard, GroupStoresCard, StateEditorModal],
  template: `
    @if (groupQuery.isPending()) {
      <qt-loading-state message="Loading group..." class="mt-12" />
    } @else if (groupQuery.isError()) {
      <div class="flex min-h-[50vh] items-center justify-center">
        <div class="text-center">
          <p class="text-lg qt-text-destructive mb-4">Error: {{ loadErrorMessage() }}</p>
          <button type="button" class="qt-text-primary hover:text-primary/80" (click)="goBack()">
            Back to groups
          </button>
        </div>
      </div>
    } @else {
      <div class="qt-page-container text-foreground">
        <div class="flex items-center gap-3 mb-6">
          <button
            type="button"
            class="inline-flex items-center justify-center rounded-lg border qt-border-default qt-bg-muted/70 p-2 qt-shadow-sm transition hover:qt-bg-muted"
            title="Go back"
            aria-label="Go back"
            (click)="goBack()"
          >
            <qt-icon name="chevron-left" class="w-5 h-5" />
          </button>
          <h1 class="qt-page-title">Edit Group</h1>
        </div>

        <form (ngSubmit)="save()" class="max-w-2xl space-y-6">
          <div>
            <label class="block text-sm font-medium text-foreground mb-2" for="qt-group-name">
              Group Name *
            </label>
            <input
              id="qt-group-name"
              name="name"
              type="text"
              [(ngModel)]="name"
              class="w-full px-4 py-2 rounded-lg border qt-border-default bg-transparent text-foreground text-sm"
              required
            />
          </div>

          <div>
            <label class="block text-sm font-medium text-foreground mb-2" for="qt-group-desc">
              Description
            </label>
            <textarea
              id="qt-group-desc"
              name="description"
              [(ngModel)]="description"
              rows="3"
              placeholder="Optional description of this group"
              class="w-full px-4 py-2 rounded-lg border qt-border-default bg-transparent text-foreground text-sm resize-none"
            ></textarea>
          </div>

          <div>
            <label class="block text-sm font-medium text-foreground mb-2" for="qt-group-color-hex">
              Color
            </label>
            <div class="flex items-center gap-3">
              <input
                type="color"
                name="colorSwatch"
                [ngModel]="color() || '#808080'"
                (ngModelChange)="color.set($event)"
                aria-label="Group color"
                class="w-12 h-10 rounded-lg cursor-pointer"
              />
              <input
                id="qt-group-color-hex"
                name="color"
                type="text"
                [(ngModel)]="color"
                placeholder="#000000"
                class="flex-1 px-4 py-2 rounded-lg border qt-border-default bg-transparent text-foreground text-sm"
              />
            </div>
          </div>

          <div>
            <label class="block text-sm font-medium text-foreground mb-2" for="qt-group-icon">
              Icon (emoji)
            </label>
            <input
              id="qt-group-icon"
              name="icon"
              type="text"
              [(ngModel)]="icon"
              maxlength="2"
              placeholder="e.g., 👥 or 🎭"
              class="w-full px-4 py-2 rounded-lg border qt-border-default bg-transparent text-foreground text-sm"
            />
          </div>

          <div class="flex gap-2">
            <button
              type="submit"
              class="qt-button qt-button-primary inline-flex items-center rounded-lg px-6 py-2 font-semibold qt-shadow-sm disabled:cursor-not-allowed disabled:opacity-60"
              [disabled]="saving()"
            >
              {{ saving() ? 'Saving...' : 'Save Changes' }}
            </button>
            <button
              type="button"
              class="qt-button qt-button-secondary inline-flex items-center rounded-lg px-6 py-2 font-semibold qt-shadow-sm"
              (click)="showStateModal.set(true)"
            >
              Group State
            </button>
          </div>
        </form>

        <div class="mt-12 space-y-6 max-w-2xl">
          <qt-group-members-card
            [members]="members()"
            [allCharacters]="allCharacters()"
            [adding]="memberBusy()"
            [removing]="memberRemoving()"
            (add)="onAddMember($event)"
            (remove)="onRemoveMember($event)"
          />
          <qt-group-stores-card
            [stores]="stores()"
            [unlinking]="storeUnlinking()"
            (unlink)="onUnlinkStore($event)"
          />
        </div>

        @if (showStateModal()) {
          <qt-state-editor-modal
            entityType="group"
            [entityId]="id()"
            [entityName]="groupName()"
            (close)="showStateModal.set(false)"
          />
        }
      </div>
    }
  `,
})
export class GroupEditor {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly toasts = inject(ToastService);

  /**
   * In-tab drill (p4.9j2, v4 `AuroraView` `selectedGroupId`): the list supplies
   * `groupId` (wins over the route `:id`) and binds `(back)` to restore the
   * list. Null ⇒ routed `/characters/groups/:id`.
   */
  readonly groupIdInput = input<string | null>(null, { alias: 'groupId' });
  readonly back = output<void>();
  protected readonly embedded = computed(() => this.groupIdInput() != null);

  /** The `:id` (route or drill input) as a signal. */
  private readonly routeId = this.route
    ? toSignal(this.route.paramMap.pipe(map((p) => p.get('id') ?? '')), { initialValue: '' })
    : undefined;
  protected readonly id = computed(() => this.groupIdInput() ?? this.routeId?.() ?? '');

  protected readonly name = signal('');
  protected readonly description = signal('');
  protected readonly color = signal('');
  protected readonly icon = signal('');
  protected readonly saving = signal(false);

  protected readonly memberBusy = signal(false);
  protected readonly memberRemoving = signal<string | null>(null);
  protected readonly storeUnlinking = signal<string | null>(null);
  protected readonly showStateModal = signal(false);

  protected readonly groupQuery = injectQuery(() => ({
    queryKey: groupKeys.detail(this.id()),
    queryFn: (): Promise<GroupSummary> => fetchGroup(this.core, this.id()),
  }));

  private readonly membersQuery = injectQuery(() => ({
    queryKey: groupKeys.members(this.id()),
    queryFn: (): Promise<GroupMemberSummary[]> => fetchGroupMembers(this.core, this.id()),
  }));

  private readonly storesQuery = injectQuery(() => ({
    queryKey: groupKeys.stores(this.id()),
    queryFn: () => fetchGroupStores(this.core, this.id()),
  }));

  private readonly charactersQuery = injectQuery(() => ({
    queryKey: ['characters', 'list', 'group-picker'],
    queryFn: async (): Promise<GroupMemberSummary[]> => {
      const list = await fetchCharacterList(this.core);
      return list.map((c) => ({ id: c.id, name: c.name }));
    },
  }));

  protected readonly members = computed(() => this.membersQuery.data() ?? []);
  protected readonly stores = computed(() => this.storesQuery.data() ?? []);
  protected readonly allCharacters = computed(() => this.charactersQuery.data() ?? []);
  /** The loaded group's name for the State modal title (v4 `group.name`). */
  protected readonly groupName = computed(() => this.groupQuery.data()?.name ?? '');

  constructor() {
    // Seed the form from the loaded group ONCE per group id (v4 sets formData in
    // the mount effect). Guard so keystrokes aren't clobbered by re-renders.
    let seededFor: string | null = null;
    effect(() => {
      const group = this.groupQuery.data();
      if (group && seededFor !== group.id) {
        seededFor = group.id;
        this.name.set(group.name ?? '');
        this.description.set(group.description ?? '');
        this.color.set(group.color ?? '');
        this.icon.set(group.icon ?? '');
      }
    });
  }

  protected loadErrorMessage(): string {
    const err = this.groupQuery.error();
    return err instanceof Error ? err.message : 'Group not found';
  }

  protected goBack(): void {
    // Drill mode ⇒ hand control back to the list; routed ⇒ navigate.
    if (this.embedded()) {
      this.back.emit();
      return;
    }
    void this.router.navigate(['/characters']);
  }

  /** v4 `GroupDetailView.tsx:79-108` — toast only, no inline surface. */
  protected async save(): Promise<void> {
    if (this.saving()) {
      return;
    }
    this.saving.set(true);
    try {
      await updateGroup(this.core, this.id(), {
        name: this.name(),
        description: this.description() || null,
        color: this.color() || null,
        icon: this.icon() || null,
      });
      await this.queryClient.invalidateQueries({ queryKey: groupKeys.detail(this.id()) });
      await this.queryClient.invalidateQueries({ queryKey: groupKeys.list() });
      this.toasts.showSuccess('Group updated successfully!');
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to save group');
    } finally {
      this.saving.set(false);
    }
  }

  /** v4 `useGroupMembers.ts:55-74` — toast only, no inline surface. */
  protected async onAddMember(characterId: string): Promise<void> {
    this.memberBusy.set(true);
    try {
      await addGroupMember(this.core, this.id(), characterId);
      await this.queryClient.invalidateQueries({ queryKey: groupKeys.members(this.id()) });
      this.toasts.showSuccess('Member added to group!');
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to add member');
    } finally {
      this.memberBusy.set(false);
    }
  }

  /** v4 `useGroupMembers.ts:76-95` — toast only, no inline surface. */
  protected async onRemoveMember(characterId: string): Promise<void> {
    this.memberRemoving.set(characterId);
    try {
      await removeGroupMember(this.core, this.id(), characterId);
      await this.queryClient.invalidateQueries({ queryKey: groupKeys.members(this.id()) });
      this.toasts.showSuccess('Member removed from group!');
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to remove member');
    } finally {
      this.memberRemoving.set(null);
    }
  }

  /**
   * Not a census row (v4's store unlinking lives in a different, unported
   * hook), but retiring the shared `saveError` banner above would otherwise
   * leave this action's failure with no feedback at all — toast it too, for
   * consistency with its three siblings in this same component.
   */
  protected async onUnlinkStore(mountPointId: string): Promise<void> {
    this.storeUnlinking.set(mountPointId);
    try {
      await unlinkGroupStore(this.core, this.id(), mountPointId);
      await this.queryClient.invalidateQueries({ queryKey: groupKeys.stores(this.id()) });
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to unlink store');
    } finally {
      this.storeUnlinking.set(null);
    }
  }
}
