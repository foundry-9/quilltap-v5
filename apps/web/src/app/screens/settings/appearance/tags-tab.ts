import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import { QuickHideService } from '../../../quick-hide/quick-hide.service';
import {
  DEFAULT_TAG_STYLE,
  deleteTag,
  fetchSettingsTags,
  mergeWithDefaultTagStyle,
  settingsTagKeys,
  updateTagQuickHide,
  updateTagVisualStyle,
  type SettingsTag,
  type TagVisualStyle,
} from './tags.api';
import { ToastService } from '../../../ui/toast.service';

/** v4 debounces the color pickers to avoid a save per drag frame (`:114,:141`). */
const COLOR_DEBOUNCE_MS = 500;

/**
 * The global Tags surface (v4 `components/settings/tags-tab.tsx`, 479 lines),
 * mounted in Settings → Appearance exactly where v4 mounts it — the LAST card,
 * after Appearance and Avatar Settings (v4 `AppearanceTabContent.tsx:8,:48-50`).
 *
 * Two sections, both v4-verbatim in copy:
 *  - "Tag Appearance" — a picker that adds a default style to an unstyled tag,
 *    then a card per styled tag (emoji, the four text toggles, the quick-hide
 *    authoring checkbox, both colors, Remove Style).
 *  - "Tag Management" — every tag with its usage count and a delete confirm.
 *
 * ⚠ THERE IS NO CREATE FLOW, and that is v4-faithful, not an omission: v4's
 * tags tab never calls `POST /tags`. Tags are born by ASSIGNMENT elsewhere (the
 * character Tags tab's "Create <name>" affordance), which is why the empty
 * state reads "Tags are created when you assign them…". The work order's
 * "list/create/edit/delete" phrasing overstates v4; v4 is the oracle, so the
 * port follows v4.
 *
 * Feedback is v4's: a toast on every save and on every failure, with v4's
 * copy (`tags-tab.tsx:85,87,189,192,209,219`).
 */
@Component({
  selector: 'qt-settings-tags',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (tagsQuery.isPending()) {
      <div class="flex items-center justify-center py-8">
        <div class="qt-text-secondary">Loading settings...</div>
      </div>
    } @else {
      <div class="space-y-6">
        <!-- Section A — Tag Appearance -->
        <div>
          <h2 class="text-xl font-semibold mb-4">Tag Appearance</h2>
          <p class="qt-text-secondary mb-4">
            Map tags to custom emojis and colors. Tags without a custom style use the default gray
            border/background and show only the tag name.
          </p>

          <div class="flex flex-wrap items-end gap-3">
            <div class="flex-1 min-w-48">
              <label class="qt-label mb-1" for="tag-style-picker">Tag</label>
              <select
                id="tag-style-picker"
                class="qt-select w-full"
                [disabled]="tagSaving() !== null || availableForStyling().length === 0"
                (change)="onSelectTag($event)"
              >
                <option value="" [selected]="selectedTagId() === ''">Select a tag</option>
                @for (tag of availableForStyling(); track tag.id) {
                  <option [value]="tag.id" [selected]="selectedTagId() === tag.id">
                    {{ tag.name }}
                  </option>
                }
              </select>
            </div>
            <button
              type="button"
              class="qt-button-primary"
              [disabled]="!selectedTagId() || tagSaving() !== null"
              (click)="addStyle()"
            >
              Add Style
            </button>
          </div>

          @if (tagsWithStyles().length > 0) {
            <div class="grid gap-4 grid-cols-1 landscape:grid-cols-3 lg:grid-cols-4 mt-4">
              @for (tag of tagsWithStyles(); track tag.id) {
                <div
                  class="border qt-border-default rounded-lg p-4 qt-bg-card qt-shadow-sm flex flex-col"
                >
                  <div class="qt-text-primary">{{ tag.name || 'Unknown tag' }}</div>
                  <div class="qt-text-xs mt-2">Preview:</div>
                  <span
                    class="qt-tag-badge qt-tag-badge-md mt-1 self-start"
                    [class.font-bold]="styleOf(tag).bold"
                    [class.italic]="styleOf(tag).italic"
                    [class.line-through]="styleOf(tag).strikethrough"
                    [style.color]="styleOf(tag).foregroundColor"
                    [style.borderColor]="styleOf(tag).foregroundColor"
                    [style.background]="styleOf(tag).backgroundColor"
                  >
                    @if (styleOf(tag).emoji) {
                      <span class="qt-tag-badge-emoji" aria-hidden="true">{{
                        styleOf(tag).emoji
                      }}</span>
                    }
                    @if (!(styleOf(tag).emojiOnly && styleOf(tag).emoji)) {
                      {{ tag.name }}
                    }
                  </span>

                  <div class="space-y-3 mt-3">
                    <label class="block qt-text-label text-foreground">
                      Emoji
                      <input
                        type="text"
                        maxlength="8"
                        class="qt-input mt-1 block w-full text-sm"
                        placeholder="😀"
                        [value]="styleOf(tag).emoji ?? ''"
                        [disabled]="tagSaving() === tag.id"
                        (change)="onEmojiChange(tag, $event)"
                      />
                    </label>

                    <div class="space-y-2 pt-1">
                      <label class="flex items-center gap-2 qt-text-label text-foreground">
                        <input
                          type="checkbox"
                          class="rounded"
                          [checked]="styleOf(tag).emojiOnly"
                          [disabled]="tagSaving() === tag.id || !styleOf(tag).emoji"
                          (change)="onFlagChange(tag, 'emojiOnly', $event)"
                        />
                        <span>Show emoji only</span>
                      </label>
                      <label class="flex items-center gap-2 qt-text-label text-foreground">
                        <input
                          type="checkbox"
                          class="rounded"
                          [checked]="styleOf(tag).bold"
                          [disabled]="tagSaving() === tag.id"
                          (change)="onFlagChange(tag, 'bold', $event)"
                        />
                        <span class="font-bold">Bold</span>
                      </label>
                      <label class="flex items-center gap-2 qt-text-label text-foreground">
                        <input
                          type="checkbox"
                          class="rounded"
                          [checked]="styleOf(tag).italic"
                          [disabled]="tagSaving() === tag.id"
                          (change)="onFlagChange(tag, 'italic', $event)"
                        />
                        <span class="italic">Italic</span>
                      </label>
                      <label class="flex items-center gap-2 qt-text-label text-foreground">
                        <input
                          type="checkbox"
                          class="rounded"
                          [checked]="styleOf(tag).strikethrough"
                          [disabled]="tagSaving() === tag.id"
                          (change)="onFlagChange(tag, 'strikethrough', $event)"
                        />
                        <span class="line-through">Strikethrough</span>
                      </label>
                    </div>

                    <!-- The quick-hide authoring bit (v4 :368-382). Gated ONLY
                         by its own in-flight save, never by tagSaving. -->
                    <div class="pt-2 mt-2 border-t border-dashed qt-border-default">
                      <label class="flex items-center gap-2 qt-text-label text-foreground">
                        <input
                          type="checkbox"
                          class="rounded"
                          [checked]="!!tag.quickHide"
                          [disabled]="quickHideSavingId() === tag.id"
                          (change)="onQuickHideToggle(tag, $event)"
                        />
                        <span>Enable quick-hide button</span>
                      </label>
                      <p class="qt-text-xs mt-1">
                        Adds this tag to the navbar quick-hide controls.
                      </p>
                    </div>

                    <label class="block qt-text-label text-foreground">
                      Border + Font Color
                      <input
                        type="color"
                        class="qt-input mt-1 block h-10 w-full"
                        [value]="styleOf(tag).foregroundColor"
                        [disabled]="tagSaving() === tag.id"
                        (input)="onColorChange(tag, 'foregroundColor', $event)"
                      />
                    </label>
                    <label class="block qt-text-label text-foreground">
                      Background Color
                      <input
                        type="color"
                        class="qt-input mt-1 block h-10 w-full"
                        [value]="styleOf(tag).backgroundColor"
                        [disabled]="tagSaving() === tag.id"
                        (input)="onColorChange(tag, 'backgroundColor', $event)"
                      />
                    </label>

                    <button
                      type="button"
                      class="w-full px-3 py-1.5 text-sm rounded-md qt-text-destructive border qt-border-destructive/30 hover:qt-bg-destructive/10 disabled:opacity-50"
                      [disabled]="tagSaving() === tag.id"
                      (click)="removeStyle(tag)"
                    >
                      Remove Style
                    </button>
                  </div>
                </div>
              }
            </div>
          } @else {
            <div class="qt-text-small border border-dashed qt-border-default rounded-lg p-4 mt-4">
              No custom tag styles yet. Select a tag above to add an emoji and colors.
            </div>
          }
        </div>

        <!-- Section B — Tag Management -->
        <div>
          <h2 class="text-xl font-semibold mb-4">Tag Management</h2>
          <p class="qt-text-secondary mb-4">
            View all tags and their usage across your workspace. Delete tags you no longer need —
            they will be removed from all entities that use them.
          </p>

          @if (sortedTags().length > 0) {
            <div class="border qt-border-default rounded-lg divide-y divide-border">
              @for (tag of sortedTags(); track tag.id) {
                <div class="flex items-center justify-between px-4 py-3">
                  <span class="qt-tag-badge qt-tag-badge-md">{{ tag.name }}</span>
                  <div class="flex items-center gap-3">
                    <span class="text-sm qt-text-secondary whitespace-nowrap">{{
                      usageLabel(tag)
                    }}</span>
                    @if (deleteConfirming() === tag.id) {
                      <div class="flex items-center gap-2">
                        <span class="text-sm text-foreground">{{ deleteMessage(tag) }}</span>
                        <button
                          type="button"
                          class="qt-button-secondary qt-button-sm"
                          [disabled]="deleting()"
                          (click)="deleteConfirming.set(null)"
                        >
                          Cancel
                        </button>
                        <button
                          type="button"
                          class="qt-button-destructive qt-button-sm"
                          [disabled]="deleting()"
                          (click)="confirmDelete(tag)"
                        >
                          {{ deleting() ? 'Deleting...' : 'Delete' }}
                        </button>
                      </div>
                    } @else {
                      <button
                        type="button"
                        class="px-3 py-1.5 text-sm rounded-md qt-text-destructive border qt-border-destructive/30 hover:qt-bg-destructive/10 disabled:opacity-50"
                        [disabled]="deleting()"
                        (click)="deleteConfirming.set(tag.id)"
                      >
                        Delete
                      </button>
                    }
                  </div>
                </div>
              }
            </div>
          } @else {
            <div class="qt-text-small border border-dashed qt-border-default rounded-lg p-4">
              No tags exist yet. Tags are created when you assign them to characters, profiles, or
              other content.
            </div>
          }
        </div>
      </div>
    }
  `,
})
export class SettingsTagsCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();
  private readonly quickHide = inject(QuickHideService);

  /** v4 `tagSaving` (`:35`) — the ONE tag whose visualStyle save is in flight. */
  protected readonly tagSaving = signal<string | null>(null);
  /** v4 `quickHideSavingId` (`:38`) — tracked separately from `tagSaving` on purpose. */
  protected readonly quickHideSavingId = signal<string | null>(null);
  protected readonly selectedTagId = signal('');
  protected readonly deleteConfirming = signal<string | null>(null);
  protected readonly deleting = signal(false);

  /** v4 `colorDebounceTimers` (`:43`) — one pending save per tag. */
  private readonly colorTimers = new Map<string, ReturnType<typeof setTimeout>>();

  protected readonly tagsQuery = injectQuery(() => ({
    queryKey: settingsTagKeys.list,
    queryFn: () => fetchSettingsTags(this.core),
  }));

  private readonly tags = computed<SettingsTag[]>(() => this.tagsQuery.data() ?? []);

  /** v4 `tagsWithStyles` (`:234-237`) — server order, no client sort. */
  protected readonly tagsWithStyles = computed(() =>
    this.tags().filter((t) => t.visualStyle !== null && t.visualStyle !== undefined),
  );

  /** v4 `availableForStyling` (`:240-243`). */
  protected readonly availableForStyling = computed(() =>
    this.tags().filter((t) => t.visualStyle === null || t.visualStyle === undefined),
  );

  /** v4 `:437` — Management re-sorts a copy by name each render. */
  protected readonly sortedTags = computed(() =>
    [...this.tags()].sort((a, b) => a.name.localeCompare(b.name)),
  );

  protected styleOf(tag: SettingsTag): TagVisualStyle {
    return mergeWithDefaultTagStyle(tag.visualStyle);
  }

  /** v4 `:444-446`: "Used 1 time" / "Used 3 times" / "Unused". */
  protected usageLabel(tag: SettingsTag): string {
    const usage = tag.totalUsage ?? 0;
    return usage > 0 ? `Used ${usage} time${usage !== 1 ? 's' : ''}` : 'Unused';
  }

  /** v4 `:459-462` — the confirm copy names the blast radius when there is one. */
  protected deleteMessage(tag: SettingsTag): string {
    const usage = tag.totalUsage ?? 0;
    return usage > 0
      ? `Delete "${tag.name}"? It will be removed from ${usage} item${usage !== 1 ? 's' : ''}.`
      : `Delete "${tag.name}"?`;
  }

  protected onSelectTag(event: Event): void {
    this.selectedTagId.set((event.target as HTMLSelectElement).value);
  }

  /** v4 `handleAddTagStyle` (`:160-166`) — writes the DEFAULT style, clears the picker. */
  protected addStyle(): void {
    const tagId = this.selectedTagId();
    if (!tagId) return;
    void this.saveVisualStyle(tagId, { ...DEFAULT_TAG_STYLE });
    this.selectedTagId.set('');
  }

  /** v4 `:155-158` — removing a style is a save of `null`, with no confirmation. */
  protected removeStyle(tag: SettingsTag): void {
    void this.saveVisualStyle(tag.id, null);
  }

  /**
   * v4 `handleTagStyleFieldChange` (`:93-112`): merge the FULL style, then save
   * immediately — no debounce on the emoji or the flags.
   */
  protected onEmojiChange(tag: SettingsTag, event: Event): void {
    const raw = (event.target as HTMLInputElement).value.trim();
    this.applyStyleChange(tag, { emoji: raw || null });
  }

  protected onFlagChange(
    tag: SettingsTag,
    key: 'emojiOnly' | 'bold' | 'italic' | 'strikethrough',
    event: Event,
  ): void {
    this.applyStyleChange(tag, { [key]: (event.target as HTMLInputElement).checked });
  }

  /** v4 `handleColorChange` (`:115-145`) — the same merge, but debounced 500ms. */
  protected onColorChange(
    tag: SettingsTag,
    key: 'foregroundColor' | 'backgroundColor',
    event: Event,
  ): void {
    const value = (event.target as HTMLInputElement).value;
    const next = { ...this.styleOf(tag), [key]: value };
    this.patchLocal(tag.id, next);
    const existing = this.colorTimers.get(tag.id);
    if (existing) clearTimeout(existing);
    this.colorTimers.set(
      tag.id,
      setTimeout(() => {
        this.colorTimers.delete(tag.id);
        void this.saveVisualStyle(tag.id, next);
      }, COLOR_DEBOUNCE_MS),
    );
  }

  private applyStyleChange(tag: SettingsTag, updates: Partial<TagVisualStyle>): void {
    const next = { ...this.styleOf(tag), ...updates };
    this.patchLocal(tag.id, next);
    void this.saveVisualStyle(tag.id, next);
  }

  /**
   * v4 `updateTagVisualStyle` (`:58-91`). Note the fidelity detail: on failure
   * v4 does NOT roll the optimistic patch back — the control keeps the value
   * until the next refetch — so v5 doesn't either.
   */
  private async saveVisualStyle(tagId: string, style: TagVisualStyle | null): Promise<void> {
    this.tagSaving.set(tagId);
    try {
      const saved = await updateTagVisualStyle(this.core, tagId, style);
      this.patchLocal(tagId, saved.visualStyle ?? null);
      this.toasts.showSuccess('Tag style saved');
    } catch (err) {
      // v4's fallback here is its generic one (`:87`), not the action's name.
      this.toasts.showError(err instanceof Error ? err.message : 'An error occurred');
    } finally {
      this.tagSaving.set(null);
    }
  }

  /**
   * v4 `handleQuickHideToggle` (`:168-198`). Two details carried deliberately:
   * there is NO optimistic update (local state is patched from the response
   * only), and the quick-hide service is refreshed on success (`:188`) so the
   * menu list picks the tag up live.
   */
  protected async onQuickHideToggle(tag: SettingsTag, event: Event): Promise<void> {
    const next = (event.target as HTMLInputElement).checked;
    this.quickHideSavingId.set(tag.id);
    try {
      const saved = await updateTagQuickHide(this.core, tag.id, next);
      this.patchQuickHideLocal(tag.id, !!saved.quickHide);
      await this.quickHide.refresh();
      this.toasts.showSuccess('Quick-hide setting saved');
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to update quick-hide');
    } finally {
      // v4's functional guard (`:194`): a slow response must not clear a
      // NEWER row's saving id.
      this.quickHideSavingId.update((cur) => (cur === tag.id ? null : cur));
    }
  }

  /**
   * v4 `handleDeleteTag` (`:200-223`) — no optimistic removal; the row goes
   * when the refetch lands, and the quick-hide list refreshes with it (`:216`).
   */
  protected async confirmDelete(tag: SettingsTag): Promise<void> {
    this.deleting.set(true);
    try {
      await deleteTag(this.core, tag.id);
      this.deleteConfirming.set(null);
      await Promise.all([
        this.queryClient.invalidateQueries({ queryKey: settingsTagKeys.list }),
        this.quickHide.refresh(),
      ]);
      this.toasts.showSuccess('Tag deleted');
    } catch (err) {
      // v4 leaves the popover open on failure.
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to delete tag');
    } finally {
      this.deleting.set(false);
    }
  }

  /** Patch one tag's visualStyle in the query cache (v4's `setTagOptions`). */
  private patchLocal(tagId: string, style: Partial<TagVisualStyle> | null): void {
    this.queryClient.setQueryData<SettingsTag[]>(settingsTagKeys.list, (prev) =>
      (prev ?? []).map((t) => (t.id === tagId ? { ...t, visualStyle: style } : t)),
    );
  }

  private patchQuickHideLocal(tagId: string, quickHide: boolean): void {
    this.queryClient.setQueryData<SettingsTag[]>(settingsTagKeys.list, (prev) =>
      (prev ?? []).map((t) => (t.id === tagId ? { ...t, quickHide } : t)),
    );
  }
}
