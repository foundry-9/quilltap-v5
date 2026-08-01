import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';

import type { WardrobeItemDto, WardrobeSlotType } from '../../../core/core-contract';
import { MarkdownField } from '../../../editor/markdown-field';
import {
  WARDROBE_SLOT_TYPES,
  type WardrobeCreateInput,
  type WardrobeMutator,
} from '../wardrobe.api';

/** The editable draft (v4 `DraftState`). */
interface DraftState {
  title: string;
  description: string;
  imagePrompt: string;
  types: WardrobeSlotType[];
  appropriateness: string;
  isDefault: boolean;
  replace: boolean;
  componentItemIds: string[];
}

const EMPTY_DRAFT: DraftState = {
  title: '',
  description: '',
  imagePrompt: '',
  types: ['top'],
  appropriateness: '',
  isDefault: false,
  replace: false,
  componentItemIds: [],
};

function draftFromItem(item: WardrobeItemDto): DraftState {
  return {
    title: item.title,
    description: item.description ?? '',
    imagePrompt: item.imagePrompt ?? '',
    types: item.types.slice(),
    appropriateness: item.appropriateness ?? '',
    isDefault: item.isDefault ?? false,
    replace: item.replace ?? false,
    componentItemIds: item.componentItemIds?.slice() ?? [],
  };
}

/**
 * ProjectWardrobeManager (v4 `components/wardrobe/ProjectWardrobeManager.tsx`) —
 * the self-contained CRUD body for a project's `Wardrobe/` folder. Renders its
 * own inline draft form + rows; does NOT use the character wardrobe-control
 * dialog family. Supports leaf garments and composites (bundling existing
 * project items); cycle rejection is server-side.
 *
 * The Description is a `qt-markdown-field` (v4 `wardrobe-item-editor.tsx:674`,
 * its `minHeight="10rem"`). v4 passes no `remountKey` there, so neither do we —
 * the draft form is seeded per open.
 */
@Component({
  selector: 'qt-project-wardrobe-manager',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MarkdownField],
  template: `
    <div class="space-y-3">
      @if (actionError(); as msg) {
        <div class="qt-alert-error" role="alert">{{ msg }}</div>
      }
      @if (mutator().error(); as msg) {
        <div class="qt-alert-error" role="alert">{{ msg }}</div>
      }

      @if (!formOpen()) {
        <div class="flex justify-end">
          <button
            type="button"
            class="qt-button qt-button-primary qt-button-sm"
            (click)="openCreate()"
          >
            + New wardrobe item
          </button>
        </div>
      }

      @if (formOpen()) {
        <div class="qt-bg-muted qt-border qt-border-default rounded p-4 space-y-3">
          <div>
            <label class="qt-label block mb-1">Title</label>
            <input
              type="text"
              class="qt-input w-full"
              placeholder="e.g. House livery jacket"
              [value]="draft().title"
              (input)="patch({ title: $any($event.target).value })"
            />
          </div>

          <div>
            <label class="qt-label block mb-1">Description</label>
            <qt-markdown-field
              ariaLabel="Wardrobe item description"
              minHeight="10rem"
              placeholder="What it looks like / how it's worn"
              [value]="draft().description"
              (contentChange)="patch({ description: $event })"
            />
          </div>

          <div>
            <label class="qt-label block mb-1">Portrait Cue (optional)</label>
            <input
              type="text"
              class="qt-input w-full"
              maxlength="200"
              placeholder="e.g. intricate burnished-gold circular rank glyph on the shoulder"
              [value]="draft().imagePrompt"
              (input)="patch({ imagePrompt: $any($event.target).value })"
            />
            <p class="mt-1 qt-text-xs qt-text-secondary">
              A short, literal phrase whispered to the portraitist and the Lantern in place of the
              title, should the bare name fail to conjure the right picture. The Description above is
              for human eyes and never reaches the easel. Leave it empty to let the title speak.
            </p>
          </div>

          <div>
            <label class="qt-label block mb-1">Slots covered</label>
            <div class="flex flex-wrap gap-3">
              @for (type of slotTypes; track type) {
                <label class="flex items-center gap-1.5 qt-text-small">
                  <input
                    type="checkbox"
                    class="qt-checkbox"
                    [checked]="draft().types.includes(type)"
                    (change)="toggleType(type)"
                  />
                  {{ type }}
                </label>
              }
            </div>
          </div>

          <div>
            <label class="qt-label block mb-1">Appropriateness (optional)</label>
            <input
              type="text"
              class="qt-input w-full"
              placeholder="e.g. formal, on-duty"
              [value]="draft().appropriateness"
              (input)="patch({ appropriateness: $any($event.target).value })"
            />
          </div>

          <div class="flex flex-wrap gap-4">
            <label class="flex items-center gap-1.5 qt-text-small">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="draft().isDefault"
                (change)="patch({ isDefault: $any($event.target).checked })"
              />
              Worn by default by every character in this project
            </label>
            <label class="flex items-center gap-1.5 qt-text-small">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="draft().replace"
                (change)="patch({ replace: $any($event.target).checked })"
              />
              Replace slots on wear (composite)
            </label>
          </div>

          @if (componentChoices().length > 0) {
            <div>
              <label class="qt-label block mb-1">
                Bundle other project items (composite — optional)
              </label>
              <div class="max-h-40 overflow-y-auto qt-border qt-border-default rounded p-2 space-y-1">
                @for (choice of componentChoices(); track choice.id) {
                  <label class="flex items-center gap-1.5 qt-text-small">
                    <input
                      type="checkbox"
                      class="qt-checkbox"
                      [checked]="draft().componentItemIds.includes(choice.id)"
                      (change)="toggleComponent(choice.id)"
                    />
                    <span class="truncate">{{ choice.title }}</span>
                  </label>
                }
              </div>
            </div>
          }

          <div class="flex justify-end gap-2 pt-1">
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              [disabled]="saving()"
              (click)="closeForm()"
            >
              Cancel
            </button>
            <button
              type="button"
              class="qt-button qt-button-primary qt-button-sm"
              [disabled]="saving()"
              (click)="handleSave()"
            >
              {{ saving() ? 'Saving…' : editingId() ? 'Save changes' : 'Create item' }}
            </button>
          </div>
        </div>
      }

      @if (mutator().loading()) {
        <p class="qt-text-secondary text-sm">Loading project wardrobe…</p>
      } @else if (mutator().items().length === 0) {
        <p class="qt-text-secondary text-sm">{{ emptyMessage() }}</p>
      } @else {
        <ul class="divide-y qt-border-default">
          @for (item of mutator().items(); track item.id) {
            <li class="py-3 flex items-start gap-3">
              <div class="flex-1 min-w-0">
                <div class="flex items-baseline gap-2 flex-wrap">
                  <h4 class="qt-label truncate">{{ item.title }}</h4>
                  <span class="qt-text-xs qt-text-secondary truncate">{{ item.types.join(', ') }}</span>
                  @if (item.componentItemIds && item.componentItemIds.length > 0) {
                    <span class="qt-badge qt-text-secondary">Composite</span>
                  }
                  @if (item.isDefault) {
                    <span class="qt-badge qt-badge-primary">Default</span>
                  }
                  @if (item.archivedAt) {
                    <span class="qt-badge qt-text-secondary">Archived</span>
                  }
                </div>
                @if (item.imagePrompt || item.description) {
                  <p class="qt-text-small qt-text-secondary mt-1">
                    {{ item.imagePrompt || item.description }}
                  </p>
                }
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <button
                  type="button"
                  class="qt-button qt-button-ghost qt-button-sm"
                  title="Edit item"
                  (click)="openEdit(item)"
                >
                  Edit
                </button>
                <button
                  type="button"
                  class="qt-button qt-button-ghost qt-button-sm qt-text-destructive"
                  title="Delete item"
                  (click)="handleDelete(item)"
                >
                  Delete
                </button>
              </div>
            </li>
          }
        </ul>
      }
    </div>
  `,
})
export class ProjectWardrobeManager {
  readonly mutator = input.required<WardrobeMutator>();
  readonly emptyMessage = input<string>(
    "No project wardrobe yet. Add a garment and every character in this project's chats can wear it.",
  );

  protected readonly slotTypes = WARDROBE_SLOT_TYPES;

  protected readonly editingId = signal<string | null>(null);
  protected readonly creating = signal(false);
  protected readonly draft = signal<DraftState>(EMPTY_DRAFT);
  protected readonly saving = signal(false);
  protected readonly actionError = signal<string | null>(null);

  protected readonly formOpen = computed(() => this.creating() || this.editingId() !== null);

  /** The composite picker excludes the item being edited (v4 `componentChoices`). */
  protected readonly componentChoices = computed(() =>
    this.mutator()
      .items()
      .filter((i) => i.id !== this.editingId()),
  );

  protected patch(part: Partial<DraftState>): void {
    this.draft.update((d) => ({ ...d, ...part }));
  }

  protected openCreate(): void {
    this.editingId.set(null);
    this.creating.set(true);
    this.draft.set(EMPTY_DRAFT);
    this.actionError.set(null);
  }

  protected openEdit(item: WardrobeItemDto): void {
    this.creating.set(false);
    this.editingId.set(item.id);
    this.draft.set(draftFromItem(item));
    this.actionError.set(null);
  }

  protected closeForm(): void {
    this.creating.set(false);
    this.editingId.set(null);
    this.draft.set(EMPTY_DRAFT);
    this.actionError.set(null);
  }

  protected toggleType(type: WardrobeSlotType): void {
    this.draft.update((d) => {
      const has = d.types.includes(type);
      const next = has ? d.types.filter((t) => t !== type) : [...d.types, type];
      // Always keep at least one slot selected.
      return { ...d, types: next.length > 0 ? next : d.types };
    });
  }

  protected toggleComponent(id: string): void {
    this.draft.update((d) => {
      const has = d.componentItemIds.includes(id);
      return {
        ...d,
        componentItemIds: has
          ? d.componentItemIds.filter((c) => c !== id)
          : [...d.componentItemIds, id],
      };
    });
  }

  protected async handleSave(): Promise<void> {
    const d = this.draft();
    const title = d.title.trim();
    if (!title) {
      this.actionError.set('Title is required.');
      return;
    }
    if (d.types.length === 0) {
      this.actionError.set('Choose at least one slot.');
      return;
    }
    this.saving.set(true);
    this.actionError.set(null);
    // Blank optional strings ride as null (v4 `handleSave`).
    const payload: WardrobeCreateInput = {
      title,
      description: d.description.trim() || null,
      imagePrompt: d.imagePrompt.trim() || null,
      types: d.types,
      appropriateness: d.appropriateness.trim() || null,
      isDefault: d.isDefault,
      replace: d.replace,
      componentItemIds: d.componentItemIds,
    };
    const editingId = this.editingId();
    const result = editingId
      ? await this.mutator().updateItem(editingId, payload)
      : await this.mutator().createItem(payload);
    this.saving.set(false);
    if (!result.ok) {
      this.actionError.set(result.error);
      return;
    }
    this.closeForm();
  }

  protected async handleDelete(item: WardrobeItemDto): Promise<void> {
    if (
      !window.confirm(`Delete project wardrobe item "${item.title}"? This cannot be undone.`)
    ) {
      return;
    }
    this.actionError.set(null);
    const result = await this.mutator().deleteItem(item.id);
    if (!result.ok) {
      this.actionError.set(result.error);
    }
  }
}
