import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterTagDetail, TagDto } from '../../../../core/core-contract';
import { Icon } from '../../../../ui/icon';
import { characterKeys, fetchCharacterTags, fetchTags, tagKeys } from '../../characters.api';

/**
 * The Tags tab (v4 `components/TagsTab.tsx` → `components/tags/tag-editor.tsx`):
 * tag chips over `characterGetTags`, add/remove via `characterAddTag` /
 * `characterRemoveTag`, and a suggestion list from `tagList` with an inline
 * "Create <name>" fallback (`tagCreate` then attach). Copy carries over
 * verbatim.
 */
@Component({
  selector: 'qt-character-tags-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="space-y-4">
      <div>
        <h2 class="qt-heading-4 text-foreground">Character Tags</h2>
        <p class="qt-text-small">
          Tags help organize and categorize this character. They can also be used for filtering and searching.
        </p>
      </div>
      <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6 space-y-2">
        <label class="block text-sm qt-text-primary">Tags</label>
        <div class="inline-flex flex-wrap gap-2 w-auto">
          @for (tag of tags(); track tag.id) {
            <span
              class="inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-sm font-medium qt-bg-muted text-foreground"
            >
              {{ tag.name }}
              <button
                type="button"
                class="qt-text-secondary hover:text-foreground disabled:opacity-50"
                [disabled]="busy()"
                [attr.aria-label]="'Remove tag ' + tag.name"
                (click)="removeTag(tag.id)"
              >
                <qt-icon name="close" class="w-3 h-3" />
              </button>
            </span>
          }

          @if (!isAdding()) {
            <button
              type="button"
              [disabled]="busy()"
              class="inline-flex items-center px-3 py-1 rounded-full text-sm font-medium qt-bg-muted text-foreground qt-hover-accent disabled:opacity-50"
              (click)="startAdding()"
            >
              + Add Tag
            </button>
          } @else {
            <div class="relative inline-flex items-center gap-1">
              <input
                type="text"
                class="qt-input py-1 px-3"
                placeholder="Add a tag..."
                [disabled]="busy()"
                [value]="inputValue()"
                (input)="inputValue.set($any($event.target).value)"
                (keydown.enter)="onEnter()"
                (keydown.escape)="cancelAdding()"
              />
              <button
                type="button"
                class="inline-flex items-center justify-center w-5 h-5 qt-text-secondary hover:text-foreground disabled:opacity-50"
                [disabled]="busy()"
                aria-label="Cancel adding tag"
                (click)="cancelAdding()"
              >
                <qt-icon name="close" class="w-4 h-4" />
              </button>

              @if (filteredSuggestions().length > 0 || inputValue().trim()) {
                <div
                  class="absolute z-10 top-full left-0 mt-1 qt-bg-card border qt-border-default rounded-md qt-shadow-lg max-h-60 overflow-y-auto"
                >
                  @if (filteredSuggestions().length > 0) {
                    <ul class="py-1">
                      @for (suggestion of filteredSuggestions(); track suggestion.id) {
                        <li>
                          <button
                            type="button"
                            [disabled]="busy()"
                            class="w-full px-4 py-2 text-left qt-text-small qt-hover-accent text-foreground disabled:opacity-50 whitespace-nowrap"
                            (click)="addExisting(suggestion)"
                          >
                            {{ suggestion.name }}
                          </button>
                        </li>
                      }
                    </ul>
                  } @else if (inputValue().trim()) {
                    <div class="py-2 px-4">
                      <button
                        type="button"
                        [disabled]="busy()"
                        class="text-left qt-text-small text-primary hover:text-primary/80 disabled:opacity-50 whitespace-nowrap"
                        (click)="createAndAdd(inputValue())"
                      >
                        Create "{{ inputValue().trim() }}"
                      </button>
                    </div>
                  }
                </div>
              }
            </div>
          }
        </div>
        @if (isAdding()) {
          <p class="qt-text-xs">Press Enter to add a tag, or select from suggestions. Press Esc to cancel.</p>
        }
      </div>
    </div>
  `,
})
export class CharacterTagsTab {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  readonly characterId = input.required<string>();

  protected readonly isAdding = signal(false);
  protected readonly inputValue = signal('');
  protected readonly busy = signal(false);

  private readonly tagsQuery = injectQuery(() => ({
    queryKey: characterKeys.tags(this.characterId()),
    queryFn: (): Promise<CharacterTagDetail[]> => fetchCharacterTags(this.core, this.characterId()),
  }));

  private readonly allTagsQuery = injectQuery(() => ({
    queryKey: tagKeys.list(),
    queryFn: (): Promise<TagDto[]> => fetchTags(this.core),
  }));

  protected readonly tags = computed(() => this.tagsQuery.data() ?? []);

  protected readonly filteredSuggestions = computed(() => {
    const q = this.inputValue().toLowerCase();
    const attached = new Set(this.tags().map((t) => t.id));
    return (this.allTagsQuery.data() ?? []).filter(
      (t) => !attached.has(t.id) && t.name.toLowerCase().includes(q),
    );
  });

  protected startAdding(): void {
    this.isAdding.set(true);
    this.inputValue.set('');
  }

  protected cancelAdding(): void {
    this.isAdding.set(false);
    this.inputValue.set('');
  }

  protected onEnter(): void {
    const match = this.filteredSuggestions()[0];
    if (match) {
      void this.addExisting(match);
    } else if (this.inputValue().trim()) {
      void this.createAndAdd(this.inputValue());
    }
  }

  protected async addExisting(tag: TagDto): Promise<void> {
    await this.attach(tag.id);
  }

  protected async createAndAdd(name: string): Promise<void> {
    const trimmed = name.trim();
    if (!trimmed || this.busy()) return;
    this.busy.set(true);
    try {
      const created = await this.core.dispatchData({ type: 'tagCreate', name: trimmed });
      const tag = (created['tag'] ?? created) as TagDto;
      await this.core.dispatchData({ type: 'characterAddTag', characterId: this.characterId(), tagId: tag.id });
      await Promise.all([
        this.queryClient.invalidateQueries({ queryKey: characterKeys.tags(this.characterId()) }),
        this.queryClient.invalidateQueries({ queryKey: tagKeys.list() }),
      ]);
      this.cancelAdding();
    } finally {
      this.busy.set(false);
    }
  }

  private async attach(tagId: string): Promise<void> {
    if (this.busy()) return;
    this.busy.set(true);
    try {
      await this.core.dispatchData({ type: 'characterAddTag', characterId: this.characterId(), tagId });
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.tags(this.characterId()) });
      this.cancelAdding();
    } finally {
      this.busy.set(false);
    }
  }

  protected async removeTag(tagId: string): Promise<void> {
    if (this.busy()) return;
    this.busy.set(true);
    try {
      await this.core.dispatchData({ type: 'characterRemoveTag', characterId: this.characterId(), tagId });
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.tags(this.characterId()) });
    } finally {
      this.busy.set(false);
    }
  }
}
