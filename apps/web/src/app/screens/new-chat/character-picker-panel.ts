import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import type { CharacterListItem } from '../../core/core-contract';
import { Avatar } from '../../ui/avatar';
import { Icon } from '../../ui/icon';
import { characterAvatarSrc } from '../characters/characters.api';
import { applyProfileChange, seedSelectedCharacter, sortRoster } from './new-chat.logic';
import { NewChatState } from './new-chat.state';
import { USER_CONTROLLED_PROFILE, type NewChatSelectedCharacter } from './new-chat.types';

/**
 * The New-Chat character picker (v4 `components/new-chat/CharacterPickerPanel.tsx`):
 * a searchable roster on the left, the selected cast with per-character
 * connection-profile + system-prompt selects on the right. The profile select's
 * special "Play As (User)" option ({@link USER_CONTROLLED_PROFILE}) flips a cast
 * entry to the human in place. Selecting/removing a character resets the chosen
 * character scenario (`scenarioId`) but leaves path selections alone.
 */
@Component({
  selector: 'qt-new-chat-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, Avatar, Icon],
  template: `
    <div class="new-chat-character-picker grid grid-cols-1 gap-6 lg:grid-cols-2 items-stretch">
      <!-- Roster -->
      <div class="flex flex-col rounded-xl border qt-border-default qt-bg-card p-6">
        <h2 class="mb-4 qt-section-title">Select Characters</h2>
        <div class="mb-4 flex-shrink-0">
          <input
            type="text"
            placeholder="Search characters..."
            [ngModel]="search()"
            (ngModelChange)="search.set($event)"
            class="w-full rounded-lg border qt-border-default bg-background px-4 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          />
        </div>
        <div class="space-y-2 overflow-y-auto" [style.height.px]="listHeightPx()">
          @if (filtered().length === 0) {
            <div class="py-8 text-center qt-text-small">
              {{ search() ? 'No characters match your search' : 'No characters available' }}
            </div>
          } @else {
            @for (character of filtered(); track character.id) {
              <button
                type="button"
                (click)="toggle(character)"
                [disabled]="disabled()"
                [class]="rowClass(isSelected(character.id))"
              >
                <qt-avatar [name]="character.name" [src]="avatarSrc(character)" size="sm" />
                <div class="flex-1 text-left">
                  <div class="qt-text-primary">{{ character.name }}</div>
                  @if (character.title) {
                    <div class="qt-text-small">{{ character.title }}</div>
                  }
                </div>
                <div [class]="checkClass(isSelected(character.id))">
                  @if (isSelected(character.id)) {
                    <qt-icon name="check" class="h-4 w-4" />
                  }
                </div>
              </button>
            }
          }
        </div>
      </div>

      <!-- Selected cast -->
      <div class="rounded-xl border qt-border-default qt-bg-card p-6">
        <h2 class="mb-4 qt-section-title">Selected Characters ({{ selected().length }})</h2>
        @if (selected().length === 0) {
          <div class="py-8 text-center qt-text-small">
            Click on characters to add them to the chat
          </div>
        } @else {
          <div class="space-y-4">
            @for (sc of selected(); track sc.character.id; let index = $index) {
              <div class="rounded-lg border qt-border-default qt-bg-muted/30 p-4">
                <div class="flex items-start gap-3">
                  <qt-avatar [name]="sc.character.name" [src]="avatarSrc(sc.character)" size="sm" />
                  <div class="flex-1">
                    <div class="flex items-center gap-2">
                      <span class="qt-text-primary">{{ sc.character.name }}</span>
                      @if (index === 0) {
                        <span
                          class="rounded qt-bg-primary/20 px-2 py-0.5 text-xs font-medium text-primary"
                          >Speaks First</span
                        >
                      }
                    </div>
                    @if (sc.character.title) {
                      <div class="qt-text-small">{{ sc.character.title }}</div>
                    }
                    <div class="mt-3">
                      <label class="mb-1 block text-xs font-medium qt-text-xs"
                        >Connection Profile</label
                      >
                      <select
                        [ngModel]="profileValue(sc)"
                        (ngModelChange)="onProfileChange(sc.character.id, $event)"
                        [disabled]="disabled()"
                        class="w-full rounded-lg border qt-border-default bg-background px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                      >
                        <option value="">Select profile...</option>
                        <option [value]="USER_CONTROLLED_PROFILE">Play As (User)</option>
                        @for (p of profiles(); track p.id) {
                          <option [value]="p.id" [selected]="p.id === profileValue(sc)">
                            {{ p.name }}{{ p.modelName ? ' (' + p.modelName + ')' : '' }}
                          </option>
                        }
                      </select>
                    </div>
                    @if (sc.character.systemPrompts && sc.character.systemPrompts.length > 0) {
                      <div class="mt-2">
                        <label class="mb-1 block text-xs font-medium qt-text-xs"
                          >System Prompt</label
                        >
                        <select
                          [ngModel]="sc.selectedSystemPromptId || ''"
                          (ngModelChange)="onPromptChange(sc.character.id, $event)"
                          [disabled]="disabled()"
                          class="w-full rounded-lg border qt-border-default bg-background px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                        >
                          <option value="">Use Default</option>
                          @for (prompt of sc.character.systemPrompts; track prompt.id) {
                            <option
                              [value]="prompt.id"
                              [selected]="prompt.id === (sc.selectedSystemPromptId || '')"
                            >
                              {{ prompt.name }}{{ prompt.isDefault ? ' (Default)' : '' }}
                            </option>
                          }
                        </select>
                      </div>
                    }
                  </div>
                  <button
                    type="button"
                    (click)="remove(sc.character.id)"
                    [disabled]="disabled()"
                    class="rounded p-1 qt-text-secondary hover:qt-bg-destructive/10 hover:qt-text-destructive"
                    title="Remove character"
                  >
                    <qt-icon name="close" class="h-5 w-5" />
                  </button>
                </div>
              </div>
            }
          </div>
        }
      </div>
    </div>
  `,
})
export class CharacterPickerPanel {
  readonly state = input.required<NewChatState>();
  readonly disabled = input(false);

  protected readonly USER_CONTROLLED_PROFILE = USER_CONTROLLED_PROFILE;
  protected readonly search = signal('');

  private core(): NewChatState {
    return this.state();
  }

  protected profiles = computed(() => this.core().profiles());
  protected selected = computed(() => this.core().selectedCharacters());

  protected readonly filtered = computed<CharacterListItem[]>(() => {
    const q = this.search().trim().toLowerCase();
    let result = this.core().characters();
    if (q) {
      result = result.filter(
        (c) => c.name.toLowerCase().includes(q) || (c.title?.toLowerCase().includes(q) ?? false),
      );
    }
    return sortRoster(result);
  });

  /** Grow the list to at least the starred count, ≥3 rows, and never fewer than selected (v4). */
  protected readonly listHeightPx = computed(() => {
    const starred = this.core()
      .characters()
      .filter((c) => c.isFavorite).length;
    const rows = Math.max(starred, 3, this.selected().length);
    return rows * 80;
  });

  protected isSelected(id: string): boolean {
    return this.selected().some((sc) => sc.character.id === id);
  }

  protected avatarSrc(c: CharacterListItem): string | null {
    return characterAvatarSrc(c.defaultImage, c.defaultImageId);
  }

  protected profileValue(sc: NewChatSelectedCharacter): string {
    return sc.controlledBy === 'user' ? USER_CONTROLLED_PROFILE : sc.connectionProfileId;
  }

  protected toggle(character: CharacterListItem): void {
    this.core().patchForm({ scenarioId: null });
    if (this.isSelected(character.id)) {
      this.core().setSelectedCharacters((prev) =>
        prev.filter((sc) => sc.character.id !== character.id),
      );
    } else {
      const seeded = seedSelectedCharacter(character, this.profiles());
      this.core().setSelectedCharacters((prev) => [...prev, seeded]);
    }
  }

  protected remove(characterId: string): void {
    this.core().patchForm({ scenarioId: null });
    this.core().setSelectedCharacters((prev) =>
      prev.filter((sc) => sc.character.id !== characterId),
    );
  }

  protected onProfileChange(characterId: string, profileId: string): void {
    this.core().setSelectedCharacters((prev) => applyProfileChange(prev, characterId, profileId));
  }

  protected onPromptChange(characterId: string, promptId: string): void {
    this.core().setSelectedCharacters((prev) =>
      prev.map((sc) =>
        sc.character.id === characterId ? { ...sc, selectedSystemPromptId: promptId || null } : sc,
      ),
    );
  }

  protected rowClass(selected: boolean): string {
    return (
      'w-full flex items-center gap-3 rounded-lg border p-3 transition ' +
      (selected
        ? 'qt-border-primary qt-bg-primary/10'
        : 'qt-border-default qt-bg-card hover:qt-border-primary/50 hover:qt-bg-muted/50')
    );
  }

  protected checkClass(selected: boolean): string {
    return (
      'flex h-6 w-6 items-center justify-center rounded-full border-2 ' +
      (selected
        ? 'qt-border-primary bg-primary text-primary-foreground'
        : 'border-muted-foreground')
    );
  }
}
