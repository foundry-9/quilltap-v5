import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ProjectDetail, ProjectRosterCharacter } from '../../../core/core-contract';
import { QuickHideService } from '../../../quick-hide/quick-hide.service';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ErrorAlert } from '../../../ui/error-alert';
import { Icon } from '../../../ui/icon';
import { Avatar } from '../../../ui/avatar';
import { characterAvatarSrc } from '../../characters/characters.api';
import { projectKeys, removeProjectCharacter, updateProject } from '../projects.api';

/**
 * The project Characters card (v4 `CharactersCard.tsx`): an "Allow Any
 * Character" toggle (immediate PUT) over a 2-col avatar roster grid with a
 * hover-X remove and avatars linking into the character view. There is NO add
 * picker — "Characters are added when chats are associated" (v4 copy).
 */
@Component({
  selector: 'qt-project-characters-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, CollapsibleCard, ErrorAlert, Icon, Avatar],
  template: `
    <qt-collapsible-card
      title="Characters"
      [description]="subtitle()"
      icon="characters"
      [defaultOpen]="defaultOpen()"
    >
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" class="mb-3" />
      }

      <div class="flex items-center justify-between px-2 py-3 qt-bg-muted rounded-lg mb-3">
        <div>
          <h4 class="qt-label text-foreground">Allow Any Character</h4>
          <p class="qt-text-xs qt-text-secondary">
            {{
              project().allowAnyCharacter
                ? 'Any character can join project chats.'
                : 'Only roster characters can participate.'
            }}
          </p>
        </div>
        <button
          type="button"
          class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors"
          [class.bg-primary]="project().allowAnyCharacter"
          [class.qt-bg-muted]="!project().allowAnyCharacter"
          role="switch"
          [attr.aria-checked]="project().allowAnyCharacter"
          aria-label="Allow Any Character"
          (click)="toggleAllowAny()"
        >
          <span
            class="inline-block h-4 w-4 transform rounded-full qt-bg-toggle-knob transition-transform"
            [class.translate-x-6]="project().allowAnyCharacter"
            [class.translate-x-1]="!project().allowAnyCharacter"
          ></span>
        </button>
      </div>

      @if (roster().length === 0) {
        <div class="text-center qt-text-secondary py-2">
          <p>No characters in the roster yet.</p>
          <p class="qt-text-small mt-1">Characters are added when chats are associated.</p>
        </div>
      } @else {
        <div class="max-h-80 overflow-y-auto">
          <div class="grid grid-cols-2 gap-2">
            @for (char of roster(); track char.id) {
              <div
                class="relative flex flex-col p-3 rounded-lg qt-border qt-bg-surface hover:qt-border-primary hover:qt-shadow-md transition-all group"
              >
                <button
                  type="button"
                  class="absolute top-1 right-1 p-1 rounded-full opacity-0 group-hover:opacity-100 qt-text-secondary hover:qt-text-destructive hover:qt-bg-destructive/10 transition-all"
                  title="Remove from roster"
                  aria-label="Remove from roster"
                  (click)="removeCharacter(char.id)"
                >
                  <qt-icon name="close" class="w-3.5 h-3.5" />
                </button>
                <a
                  [routerLink]="['/characters', char.id]"
                  class="flex flex-col items-center gap-2 hover:opacity-80 transition-opacity"
                >
                  <qt-avatar [name]="char.name || 'Unknown'" [src]="avatarSrc(char)" size="md" />
                  <div class="text-center w-full">
                    <h4 class="text-sm font-semibold text-foreground truncate px-1">
                      {{ char.name || 'Unknown Character' }}
                    </h4>
                    <p class="qt-text-xs qt-text-secondary">{{ chatLabel(char) }}</p>
                  </div>
                </a>
              </div>
            }
          </div>
        </div>
      }
    </qt-collapsible-card>
  `,
})
export class ProjectCharactersCard {
  readonly project = input.required<ProjectDetail>();
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly quickHide = inject(QuickHideService);
  protected readonly saveError = signal<string | null>(null);

  /**
   * v4 `CharactersCard.tsx:31-38`: dedupe by id (a character can appear twice
   * in the roster), then drop any carrying a hidden tag. Both the grid and the
   * "{n} characters in roster" subtitle read this filtered list, exactly as v4's
   * `visibleCharacters` feeds its own count (`:53`).
   */
  protected readonly roster = computed<ProjectRosterCharacter[]>(() => {
    const seen = new Set<string>();
    return (this.project().roster ?? []).filter((char) => {
      if (!char.id || seen.has(char.id)) return false;
      seen.add(char.id);
      return !this.quickHide.shouldHideByIds(char.tags ?? []);
    });
  });

  protected readonly subtitle = computed(() => {
    const n = this.roster().length;
    return `${n} character${n !== 1 ? 's' : ''} in roster`;
  });

  protected avatarSrc(char: ProjectRosterCharacter): string | null {
    return characterAvatarSrc(char.defaultImage, char.defaultImageId);
  }

  protected chatLabel(char: ProjectRosterCharacter): string {
    const n = char.chatCount ?? 0;
    return `${n} chat${n !== 1 ? 's' : ''}`;
  }

  protected async toggleAllowAny(): Promise<void> {
    this.saveError.set(null);
    try {
      await updateProject(this.core, this.project().id, {
        allowAnyCharacter: !this.project().allowAnyCharacter,
      });
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.project().id) });
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to update setting');
    }
  }

  protected async removeCharacter(characterId: string): Promise<void> {
    this.saveError.set(null);
    try {
      await removeProjectCharacter(this.core, this.project().id, characterId);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.project().id) });
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to remove character');
    }
  }
}
