import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { TagDto } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { ToastService } from '../../../ui/toast.service';
import { fetchTags, tagKeys } from '../characters.api';

/**
 * The character tag editor (v4 `components/tags/tag-editor.tsx`, TagEditor):
 * chips over the character's current tag ids, an "+ Add Tag" input with
 * suggestions from the workspace tag catalog (`tagList`), and Enter-to-create
 * for a brand-new name. v4 persists add/remove immediately against the
 * character's own `?action=add-tag|remove-tag` endpoints; this port instead
 * stages the id array in the parent form and folds it into the ONE
 * `characterUpdate` bag on Save (the work order's explicit simplification —
 * flagged in the port report). Creating a brand-new tag (one that doesn't yet
 * exist in the catalog) still goes through `tagCreate` immediately, since a
 * tag id has to exist before it can be staged.
 *
 * v4's two toasts (`tag-editor.tsx:140,167`, "Failed to add tag"/"Failed to
 * remove tag") guard v4's own IMMEDIATE add-tag/remove-tag persistence; this
 * port's staged add/remove never touches the network on their own (they
 * fold into the parent's `characterUpdate` Save, already toasted there), so
 * only the add side has a real failure point left — the `tagCreate` call
 * for a brand-new name, which carries v4's add-tag fallback sentence.
 */
@Component({
  selector: 'qt-tag-chip-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="space-y-2">
      <label class="block qt-label">Tags</label>
      <div class="inline-flex flex-wrap gap-2 w-auto">
        @for (tag of currentTags(); track tag.id) {
          <span
            class="inline-flex items-center gap-1 rounded-full qt-bg-muted px-3 py-1 text-sm text-foreground"
          >
            {{ tag.name }}
            <button
              type="button"
              class="ml-1 qt-text-secondary hover:text-foreground"
              [attr.aria-label]="'Remove tag ' + tag.name"
              (click)="removeTag(tag.id)"
            >
              <qt-icon name="close" class="w-3 h-3" />
            </button>
          </span>
        }

        @if (!adding()) {
          <button
            type="button"
            class="inline-flex items-center px-3 py-1 rounded-full text-sm font-medium qt-bg-muted text-foreground"
            (click)="adding.set(true)"
          >
            + Add Tag
          </button>
        } @else {
          <div class="relative inline-flex items-center gap-1">
            <input
              type="text"
              class="qt-input py-1 px-3"
              placeholder="Add a tag..."
              [value]="inputValue()"
              (input)="inputValue.set($any($event.target).value)"
              (keydown.enter)="onEnter($event)"
              (keydown.escape)="cancelAdd()"
            />
            <button
              type="button"
              class="inline-flex items-center justify-center w-5 h-5 qt-text-secondary hover:text-foreground"
              aria-label="Cancel adding tag"
              (click)="cancelAdd()"
            >
              <qt-icon name="close" class="w-4 h-4" />
            </button>

            @if (filteredSuggestions().length > 0) {
              <div
                class="absolute z-10 top-full left-0 mt-1 qt-bg-card border qt-border-default rounded-md qt-shadow-lg max-h-60 overflow-y-auto"
              >
                <ul class="py-1">
                  @for (tag of filteredSuggestions(); track tag.id) {
                    <li>
                      <button
                        type="button"
                        class="w-full px-4 py-2 text-left qt-text-small text-foreground whitespace-nowrap"
                        (click)="addExisting(tag)"
                      >
                        {{ tag.name }}
                      </button>
                    </li>
                  }
                </ul>
              </div>
            }
          </div>
        }
      </div>
      <p class="qt-text-xs">Press Enter to add a tag, or select from suggestions.</p>
    </div>
  `,
})
export class TagChipEditor {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  /** Tag ids currently on the character (the form's staged array). */
  readonly tagIds = input.required<string[]>();
  readonly tagIdsChange = output<string[]>();

  protected readonly adding = signal(false);
  protected readonly inputValue = signal('');
  protected readonly creating = signal(false);

  private readonly tagsQuery = injectQuery(() => ({
    queryKey: tagKeys.all,
    queryFn: (): Promise<TagDto[]> => fetchTags(this.core),
  }));

  private readonly allTags = computed(() => this.tagsQuery.data() ?? []);

  protected readonly currentTags = computed<TagDto[]>(() => {
    const byId = new Map(this.allTags().map((t) => [t.id, t]));
    return this.tagIds()
      .map((id) => byId.get(id))
      .filter((t): t is TagDto => !!t);
  });

  protected readonly filteredSuggestions = computed<TagDto[]>(() => {
    const query = this.inputValue().trim().toLowerCase();
    if (!query) {
      return [];
    }
    const taken = new Set(this.tagIds());
    return this.allTags().filter((t) => t.name.toLowerCase().includes(query) && !taken.has(t.id));
  });

  protected addExisting(tag: TagDto): void {
    if (this.tagIds().includes(tag.id)) {
      return;
    }
    this.tagIdsChange.emit([...this.tagIds(), tag.id]);
    this.cancelAdd();
  }

  protected removeTag(tagId: string): void {
    this.tagIdsChange.emit(this.tagIds().filter((id) => id !== tagId));
  }

  protected async onEnter(event: Event): Promise<void> {
    event.preventDefault();
    const exactMatch = this.allTags().find(
      (t) => t.name.toLowerCase() === this.inputValue().trim().toLowerCase(),
    );
    if (exactMatch) {
      this.addExisting(exactMatch);
      return;
    }
    const suggestion = this.filteredSuggestions()[0];
    if (suggestion) {
      this.addExisting(suggestion);
      return;
    }
    const name = this.inputValue().trim();
    if (!name || this.creating()) {
      return;
    }
    this.creating.set(true);
    try {
      const data = await this.core.dispatchData({ type: 'tagCreate', name });
      const tag = data['tag'] as TagDto | undefined;
      if (tag) {
        this.tagIdsChange.emit([...this.tagIds(), tag.id]);
      }
      this.cancelAdd();
    } catch {
      // v4 `tag-editor.tsx:140` — the create-tag step shares the add-tag catch.
      this.toasts.showError('Failed to add tag. Please try again.');
    } finally {
      this.creating.set(false);
    }
  }

  protected cancelAdd(): void {
    this.inputValue.set('');
    this.adding.set(false);
  }
}
