import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { EditorTag, TagDto } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { ToastService } from '../../../ui/toast.service';
import { fetchTags, tagKeys } from '../../characters/characters.api';

/** Query key for one profile's own tag details. */
export const profileTagKeys = {
  tags: (profileId: string) => ['connectionProfiles', profileId, 'tags'] as const,
};

/**
 * The connection-profile tag editor (v4 `components/tags/tag-editor.tsx` with
 * `entityType="profile"`, as fixed at `d123658d` — Bug 74's first layer was
 * that this branch reached for `/api/v1/profiles/<id>`, a route that has never
 * existed, so every read and write 404'd silently).
 *
 * v4's TagEditor persists add and remove IMMEDIATELY against the entity's own
 * endpoints, and the profile modal has no single Save bag that tags could ride.
 * So this one follows v4 rather than the character form's staged simplification
 * (`characters/edit/tag-chip-editor.ts:14-36` records that deviation): each
 * gesture goes straight out over `connectionProfileAddTag` /
 * `connectionProfileRemoveTag`, and v4's two failure toasts
 * (`tag-editor.tsx:140,167`) guard exactly that.
 *
 * The list it reads is `connectionProfileGetTags` — the FLAT `EditorTag` shape.
 * The card's pills come from the LISTING's `{ tagId, tag }` envelope instead;
 * conflating the two was Bug 74's second layer.
 */
@Component({
  selector: 'qt-profile-tag-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="space-y-2">
      <label class="block text-sm qt-text-primary">Tags</label>
      <div class="inline-flex flex-wrap gap-2 w-auto">
        @for (tag of tags(); track tag.id) {
          <span class="qt-tag-badge qt-tag-badge-md">
            <span>{{ tag.name }}</span>
            <button
              type="button"
              class="qt-tag-badge-remove"
              [disabled]="busy()"
              [attr.aria-label]="'Remove ' + tag.name + ' tag'"
              (click)="removeTag(tag.id)"
            >
              <qt-icon name="close" class="h-3 w-3" />
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
              (keydown.enter)="onEnter($event)"
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
                          (click)="addTag(suggestion.name)"
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
                      (click)="addTag(inputValue())"
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
        <p class="qt-text-xs">
          Press Enter to add a tag, or select from suggestions. Press Esc to cancel.
        </p>
      }
    </div>
  `,
})
export class ProfileTagEditor {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly toasts = inject(ToastService);

  readonly profileId = input.required<string>();

  protected readonly isAdding = signal(false);
  protected readonly inputValue = signal('');
  protected readonly busy = signal(false);

  private readonly tagsQuery = injectQuery(() => ({
    queryKey: profileTagKeys.tags(this.profileId()),
    queryFn: async (): Promise<EditorTag[]> => {
      const data = await this.core.dispatchData({
        type: 'connectionProfileGetTags',
        profileId: this.profileId(),
      });
      return (data['tags'] as EditorTag[]) ?? [];
    },
  }));

  // The workspace tag catalog, on the SAME key the character editors read, so
  // a tag created here shows up there without a second round trip.
  private readonly allTagsQuery = injectQuery(() => ({
    queryKey: tagKeys.list(),
    queryFn: (): Promise<TagDto[]> => fetchTags(this.core),
  }));

  protected readonly tags = computed(() => this.tagsQuery.data() ?? []);

  /** v4 `:88-93` — substring match on the raw input, minus what is attached. */
  protected readonly filteredSuggestions = computed(() => {
    const query = this.inputValue().toLowerCase();
    const attached = new Set(this.tags().map((t) => t.id));
    return (this.allTagsQuery.data() ?? []).filter(
      (t) => t.name.toLowerCase().includes(query) && !attached.has(t.id),
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

  /** v4 `handleKeyDown` (`:170-180`): the first suggestion wins over the raw text. */
  protected onEnter(event: Event): void {
    event.preventDefault();
    const suggestion = this.filteredSuggestions()[0];
    if (suggestion) {
      void this.addTag(suggestion.name);
    } else if (this.inputValue().trim()) {
      void this.addTag(this.inputValue());
    }
  }

  /**
   * v4 `addTag` (`:101-141`): create-or-get the tag, then attach it. Both legs
   * share ONE catch, and its toast, because a tag that exists but never reached
   * the profile is the same failure to the person who pressed Enter.
   */
  protected async addTag(tagName: string): Promise<void> {
    const name = tagName.trim();
    if (!name || this.busy()) return;
    this.busy.set(true);
    try {
      const created = await this.core.dispatchData({ type: 'tagCreate', name });
      const tag = (created['tag'] ?? created) as TagDto;
      await this.core.dispatchData({
        type: 'connectionProfileAddTag',
        profileId: this.profileId(),
        tagId: tag.id,
      });
      await Promise.all([
        this.queryClient.invalidateQueries({ queryKey: profileTagKeys.tags(this.profileId()) }),
        this.queryClient.invalidateQueries({ queryKey: tagKeys.all }),
        this.queryClient.invalidateQueries({ queryKey: ['connectionProfiles'] }),
      ]);
      this.cancelAdding();
    } catch {
      this.toasts.showError('Failed to add tag. Please try again.');
    } finally {
      this.busy.set(false);
    }
  }

  /** v4 `removeTag` (`:143-168`) — its own sentence, its own catch. */
  protected async removeTag(tagId: string): Promise<void> {
    if (this.busy()) return;
    this.busy.set(true);
    try {
      await this.core.dispatchData({
        type: 'connectionProfileRemoveTag',
        profileId: this.profileId(),
        tagId,
      });
      await Promise.all([
        this.queryClient.invalidateQueries({ queryKey: profileTagKeys.tags(this.profileId()) }),
        this.queryClient.invalidateQueries({ queryKey: ['connectionProfiles'] }),
      ]);
    } catch {
      this.toasts.showError('Failed to remove tag. Please try again.');
    } finally {
      this.busy.set(false);
    }
  }
}
