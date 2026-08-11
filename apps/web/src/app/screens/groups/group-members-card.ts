import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import { CollapsibleCard } from '../../ui/collapsible-card';
import { Icon } from '../../ui/icon';
import type { GroupMemberSummary } from '../../core/core-contract';

/**
 * The group Members card (v4 `GroupMembersCard.tsx`): a collapsed-by-default
 * expandable card with a scrollable member list (X to remove) and an "Add
 * Member" affordance that swaps in a `<select>` of non-member characters.
 *
 * The picker `<select>` binds `[selected]` PER OPTION (never `[value]` on the
 * select) — the dogfood-finding-#6 discipline for async-options selects.
 */
@Component({
  selector: 'qt-group-members-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, Icon],
  template: `
    <qt-collapsible-card title="Members" [description]="subtitle()" icon="users">
      @if (members().length === 0) {
        <div class="text-center qt-text-secondary py-2">
          <p>No members yet.</p>
          <p class="qt-text-small mt-1">Add characters to this group.</p>
        </div>
      } @else {
        <div class="max-h-64 overflow-y-auto space-y-1">
          @for (member of members(); track member.id) {
            <div
              class="flex items-center justify-between p-3 rounded-lg hover:qt-bg-muted transition-colors"
            >
              <div class="flex-1 min-w-0 flex items-center gap-2">
                <p class="qt-label truncate">{{ member.name }}</p>
                @if (member.archivedAt) {
                  <span
                    class="inline-flex flex-shrink-0 items-center rounded-full border qt-border-default qt-bg-muted px-2 py-0.5 text-xs qt-text-secondary"
                    title="Resting in the archive — still a member, but takes no turns until rehydrated"
                  >
                    Archived
                  </span>
                }
              </div>
              <button
                type="button"
                class="ml-2 flex-shrink-0 p-1.5 rounded hover:qt-text-destructive transition-colors disabled:opacity-50"
                title="Remove member"
                aria-label="Remove member"
                [disabled]="removing() === member.id"
                (click)="remove.emit(member.id)"
              >
                <qt-icon name="close" class="w-4 h-4" />
              </button>
            </div>
          }
        </div>
      }

      <div class="pt-2 mt-2 border-t qt-border-default">
        @if (!showPicker()) {
          <button
            type="button"
            class="w-full qt-button qt-button-primary text-sm"
            [disabled]="available().length === 0"
            [title]="available().length === 0 ? 'All characters are already members' : ''"
            (click)="showPicker.set(true)"
          >
            Add Member
          </button>
        } @else {
          <div class="space-y-2">
            <select
              class="w-full px-3 py-2 rounded border qt-border-default bg-transparent text-foreground text-sm"
              aria-label="Select a character to add"
              (change)="onSelect($event)"
            >
              <option value="" [selected]="selectedId() === null">Select a character...</option>
              @for (character of available(); track character.id) {
                <option [value]="character.id" [selected]="selectedId() === character.id">
                  {{ character.name }}
                </option>
              }
            </select>
            <div class="flex gap-2">
              <button
                type="button"
                class="flex-1 qt-button qt-button-primary text-sm"
                [disabled]="!selectedId() || adding()"
                (click)="onAdd()"
              >
                {{ adding() ? 'Adding...' : 'Add' }}
              </button>
              <button
                type="button"
                class="flex-1 qt-button qt-button-secondary text-sm"
                (click)="cancel()"
              >
                Cancel
              </button>
            </div>
          </div>
        }
      </div>
    </qt-collapsible-card>
  `,
})
export class GroupMembersCard {
  readonly members = input.required<GroupMemberSummary[]>();
  readonly allCharacters = input.required<GroupMemberSummary[]>();
  readonly adding = input(false);
  readonly removing = input<string | null>(null);

  readonly add = output<string>();
  readonly remove = output<string>();

  protected readonly showPicker = signal(false);
  protected readonly selectedId = signal<string | null>(null);

  protected readonly available = computed(() => {
    const memberIds = new Set(this.members().map((m) => m.id));
    return this.allCharacters().filter((c) => !memberIds.has(c.id));
  });

  /**
   * v4 `GroupMembersCard.tsx:75-81`. The archived clause appears ONLY when at
   * least one member is archived, so an ordinary group's subtitle is unchanged:
   * `3 members` stays `3 members`, and one tombstone makes it
   * `3 members / 2 can speak (1 archived)`. Membership survives archiving — the
   * seat simply takes no turns.
   */
  protected readonly subtitle = computed(() => {
    const members = this.members();
    const n = members.length;
    const archived = members.filter((m) => m.archivedAt).length;
    const base = `${n} member${n !== 1 ? 's' : ''}`;
    return archived > 0 ? `${base} / ${n - archived} can speak (${archived} archived)` : base;
  });

  protected onSelect(event: Event): void {
    const value = (event.target as HTMLSelectElement).value;
    this.selectedId.set(value || null);
  }

  protected onAdd(): void {
    const id = this.selectedId();
    if (!id) {
      return;
    }
    this.add.emit(id);
  }

  protected cancel(): void {
    this.showPicker.set(false);
    this.selectedId.set(null);
  }

  /** Called by the parent after a successful add to reset the picker. */
  reset(): void {
    this.showPicker.set(false);
    this.selectedId.set(null);
  }
}
