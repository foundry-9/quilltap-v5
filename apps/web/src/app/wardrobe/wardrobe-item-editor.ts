import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
  viewChild,
  type ElementRef,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { MarkdownField } from '../editor/markdown-field';
import { WARDROBE_SLOT_TYPES, unionTypes } from './equipped-slots';
import { dispatchWardrobe } from './wardrobe.api';
import { GROUP_ORDER, getCandidateGroup } from './item-editor/constants';
import type { CandidateGroup, CandidateItem } from './item-editor/types';
import { WardrobeComponentPicker } from './item-editor/wardrobe-component-picker';
import { WardrobeModeChangePrompt } from './item-editor/wardrobe-mode-change-prompt';

type EditorMode = 'single' | 'bundle';

/** Where a newly-created wardrobe item is written
 *  (v4 `wardrobe-item-editor.tsx:23`). */
export type WardrobeCreateScope = 'character' | 'global' | 'project';

/** v4 `lib/utils/char-count.ts` `charCountClass` — the qt-* colour class for a
 *  character-count indicator by proximity to `max`. */
function charCountClass(current: number, max: number): string {
  if (current > max) return 'qt-text-destructive';
  if (current > max * 0.9) return 'qt-text-warning';
  return 'qt-text-secondary';
}

/**
 * The tier-routed wardrobe item editor — a port of v4
 * `components/wardrobe/wardrobe-item-editor.tsx`. Single-garment vs
 * outfit-bundle modes, the composite component picker, the replace-slots
 * designation, and the five save routes chosen BY TIER (v4 `:358-369`,
 * transcribed verbatim in `saveRequest()`).
 *
 * Stacks above the wardrobe control dialog (v4 z-[70]/z-[80], `:399/:407`).
 *
 * DIVERGENCES (deliberate, recorded): v4's success/error toasts become the
 * inline `qt-alert-error` + the parent's reload (v5 has no toast system — the
 * `project-wardrobe-manager` precedent); the description markdown editor is
 * the shared `qt-markdown-field` (v4 `MarkdownLexicalEditor`,
 * `minHeight="10rem"` carried, `:679`) whose absorb-once seam keeps the
 * async-loaded content from marking the form dirty.
 */
@Component({
  selector: 'qt-wardrobe-item-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MarkdownField, WardrobeComponentPicker, WardrobeModeChangePrompt],
  template: `
    <!-- Overlay — z values sit above the qt-dialog-overlay (z-[60]) so this
         editor always stacks on top when summoned from another dialog. -->
    <button
      class="qt-dialog-overlay !p-0 cursor-default border-none z-[70]"
      (click)="closed.emit()"
      aria-label="Close dialog"
      type="button"
    ></button>

    <div
      class="fixed top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 z-[80] pointer-events-auto"
      style="width: min(var(--qt-page-max-width), calc(100vw - 2rem))"
    >
      <div class="qt-dialog qt-dialog-wide max-h-[90vh] overflow-y-auto flex flex-col">
        <div
          class="qt-dialog-header sticky top-0 flex-shrink-0 qt-bg-default border-b qt-border-default"
        >
          <div class="flex items-center justify-between">
            <h2 class="qt-dialog-title">
              {{ isEditing() ? 'Edit Wardrobe Item' : 'New Wardrobe Item' }}
            </h2>
            <button
              type="button"
              (click)="closed.emit()"
              class="qt-text-secondary hover:text-foreground"
              aria-label="Close"
            >
              ✕
            </button>
          </div>
        </div>

        <div class="qt-dialog-body space-y-4 flex-1">
          @if (saveError(); as msg) {
            <div class="qt-alert-error text-sm" role="alert">{{ msg }}</div>
          }

          <!-- Destination scope — only when creating. Editing keeps an item in
               its existing tier (v4 :429-468). -->
          @if (!isEditing()) {
            <div>
              <span class="qt-label mb-2 block">Add to</span>
              <div
                role="radiogroup"
                aria-label="Where to save this item"
                class="inline-flex flex-wrap gap-1 qt-bg-muted/50 rounded-lg p-1"
              >
                @for (opt of scopeOptions(); track opt.scope) {
                  <button
                    type="button"
                    role="radio"
                    [attr.aria-checked]="createScope() === opt.scope"
                    (click)="createScope.set(opt.scope)"
                    class="px-3 py-1.5 rounded text-sm font-medium transition-colors"
                    [class.qt-bg-default]="createScope() === opt.scope"
                    [class.text-foreground]="createScope() === opt.scope"
                    [class.shadow-sm]="createScope() === opt.scope"
                    [class.qt-text-secondary]="createScope() !== opt.scope"
                  >
                    {{ opt.label }}
                  </button>
                }
              </div>
              <p class="qt-text-xs qt-text-secondary mt-1">{{ scopeHint() }}</p>
            </div>
          }

          <!-- Mode toggle — Single garment vs. Outfit bundle (v4 :470-502). -->
          <div
            role="tablist"
            aria-label="Wardrobe item kind"
            class="inline-flex gap-1 qt-bg-muted/50 rounded-lg p-1"
          >
            <button
              type="button"
              role="tab"
              [attr.aria-selected]="editorMode() === 'single'"
              (click)="handleModeChange('single')"
              class="px-3 py-1.5 rounded text-sm font-medium transition-colors"
              [class.qt-bg-default]="editorMode() === 'single'"
              [class.text-foreground]="editorMode() === 'single'"
              [class.shadow-sm]="editorMode() === 'single'"
              [class.qt-text-secondary]="editorMode() !== 'single'"
            >
              Single garment
            </button>
            <button
              type="button"
              role="tab"
              [attr.aria-selected]="editorMode() === 'bundle'"
              (click)="handleModeChange('bundle')"
              class="px-3 py-1.5 rounded text-sm font-medium transition-colors"
              [class.qt-bg-default]="editorMode() === 'bundle'"
              [class.text-foreground]="editorMode() === 'bundle'"
              [class.shadow-sm]="editorMode() === 'bundle'"
              [class.qt-text-secondary]="editorMode() !== 'bundle'"
            >
              Outfit bundle
            </button>
          </div>

          <!-- Shared item notice (v4 :505-509). -->
          @if (isShared() && isEditing()) {
            <div
              class="rounded border qt-border-warning/50 qt-bg-warning/10 px-3 py-2 qt-text-small qt-text-warning"
            >
              Changes to shared items affect all characters
            </div>
          }

          <!-- Title (v4 :511-535). -->
          <div>
            <label for="wardrobe-title" class="qt-label mb-1">Title *</label>
            <input
              #titleInput
              type="text"
              id="wardrobe-title"
              [value]="title()"
              (input)="title.set($any($event.target).value)"
              (blur)="touchedTitle.set(true)"
              required
              [placeholder]="
                isBundle() ? 'e.g., Working Outfit, Sunday Best' : 'e.g., Charcoal Sweater'
              "
              class="qt-input"
            />
            @if (showTitleError()) {
              <p class="mt-1 text-xs qt-text-destructive">Enter a title</p>
            }
          </div>

          <!-- Single mode: Types checkboxes (v4 :537-569). -->
          @if (!isBundle()) {
            <div>
              <span class="qt-label mb-2 block">Type(s) *</span>
              <div class="flex flex-wrap gap-3" (focusout)="touchedTypes.set(true)">
                @for (type of slotTypes; track type) {
                  <label class="inline-flex items-center gap-2 cursor-pointer">
                    <input
                      type="checkbox"
                      [checked]="selectedTypes().includes(type)"
                      (change)="handleTypeToggle(type)"
                      class="qt-checkbox"
                    />
                    <span class="text-sm capitalize text-foreground">{{ type }}</span>
                  </label>
                }
              </div>
              @if (showTypesError()) {
                <p class="mt-1 text-xs qt-text-destructive">Select at least one type</p>
              }
            </div>
          }

          <!-- Bundle mode: Components section (v4 :571-593). -->
          @if (isBundle()) {
            <qt-wardrobe-component-picker
              [effectiveTypes]="effectiveTypes()"
              [selectedComponents]="selectedComponents()"
              [componentSearch]="componentSearch()"
              [candidatesLoading]="candidatesLoading()"
              [candidates]="candidates()"
              [eligibleCandidates]="eligibleCandidates()"
              [groupedCandidates]="groupedCandidates()"
              [expandedGroups]="expandedGroups()"
              [componentItemIds]="componentItemIds()"
              [replace]="replace()"
              [computedTypes]="computedTypes()"
              [showComponentsError]="showComponentsError()"
              (componentSearchChange)="componentSearch.set($event)"
              (componentsBlur)="touchedComponents.set(true)"
              (toggleComponent)="toggleComponent($event)"
              (toggleGroup)="toggleGroup($event)"
              (toggleType)="handleTypeToggle($event)"
              (replaceChange)="replace.set($event)"
            />
          }

          <!-- Default-outfit toggle (v4 :595-611). Whether an item is shared is
               governed by the "Add to" selector (create) or the item's existing
               tier (edit) — there is no separate "shared" checkbox. -->
          <div>
            <label class="inline-flex items-start gap-2 cursor-pointer">
              <input
                type="checkbox"
                [checked]="isDefault()"
                (change)="isDefault.set($any($event.target).checked)"
                class="qt-checkbox mt-0.5"
              />
              <span class="text-sm text-foreground">
                Part of this character's default outfit
              </span>
            </label>
          </div>

          <!-- Appropriateness (v4 :613-636). -->
          <div>
            <div class="flex items-center justify-between mb-1">
              <label for="wardrobe-appropriateness" class="qt-label">Appropriateness</label>
              <span class="text-xs" [class]="appropriatenessCountClass()">
                {{ appropriateness().length }}/200
              </span>
            </div>
            <input
              type="text"
              id="wardrobe-appropriateness"
              [value]="appropriateness()"
              (input)="appropriateness.set($any($event.target).value)"
              maxlength="200"
              placeholder="e.g., formal, casual, intimate, combat"
              class="qt-input"
            />
            <p class="mt-1 text-xs qt-text-small">
              When is this appropriate to wear? e.g., formal, casual, intimate, combat.
            </p>
          </div>

          <!-- Portrait cue — plain-text phrase handed to the image-makers (v4 :638-664). -->
          <div>
            <div class="flex items-center justify-between mb-1">
              <label for="wardrobe-image-prompt" class="qt-label">Portrait Cue</label>
              <span class="text-xs" [class]="imagePromptCountClass()">
                {{ imagePrompt().length }}/200
              </span>
            </div>
            <input
              type="text"
              id="wardrobe-image-prompt"
              [value]="imagePrompt()"
              (input)="imagePrompt.set($any($event.target).value)"
              maxlength="200"
              placeholder="e.g., intricate burnished-gold circular rank glyph on the shoulder"
              class="qt-input"
            />
            <p class="mt-1 text-xs qt-text-small">
              A short, literal phrase whispered to the portraitist and the Lantern when a likeness
              is drawn --- used <em>in place of</em> the title above, should the bare name fail to
              conjure the right picture. Keep it terse and visual; the flowery Description below is
              for human eyes and never reaches the easel. Leave it empty to let the title speak.
            </p>
          </div>

          <!-- Description (Markdown, v4 :666-681). -->
          <div>
            <label for="wardrobe-description" class="block text-sm qt-text-primary mb-1">
              Description
            </label>
            <p class="text-xs qt-text-secondary mb-2">Describe the item in detail.</p>
            <qt-markdown-field
              ariaLabel="Wardrobe item description"
              minHeight="10rem"
              [value]="description()"
              (contentChange)="description.set($event)"
            />
          </div>
        </div>

        <!-- Footer (v4 :684-694). -->
        <div class="qt-dialog-footer flex-shrink-0">
          <div class="flex gap-2 justify-end">
            <button
              type="button"
              class="qt-button-secondary"
              [disabled]="saving()"
              (click)="closed.emit()"
            >
              Cancel
            </button>
            <button
              type="button"
              class="qt-button-primary"
              [disabled]="isSaveDisabled() || saving()"
              (click)="handleSave()"
            >
              {{ saving() ? 'Saving…' : isEditing() ? 'Update' : 'Create' }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Bundle → Single confirmation prompt (v4 :698-706). -->
    @if (showKeepResetPrompt()) {
      <qt-wardrobe-mode-change-prompt
        [componentCount]="componentItemIds().length"
        (cancel)="showKeepResetPrompt.set(false)"
        (reset)="handleConfirmReset()"
        (keepTypes)="handleConfirmKeepTypes()"
      />
    }
  `,
})
export class WardrobeItemEditor {
  private readonly core = inject(CoreClient);

  readonly characterId = input.required<string>();
  readonly item = input<WardrobeItemDto | null>(null);
  /** Whether this item is being created/edited as a shared item. */
  readonly isSharedInput = input(false, { alias: 'isShared' });
  /** Project context (the chat's project) — when present the create-scope
   *  selector offers a "this project" destination (v4 `:30-34`). */
  readonly projectId = input<string | null>(null);
  /** Pre-populated component IDs (Save-as-outfit from the Outfit Builder). */
  readonly initialComponentItemIds = input<string[] | undefined>(undefined);
  /** Force a starting mode (Save-as-outfit opens in bundle mode). */
  readonly initialMode = input<EditorMode | undefined>(undefined);
  /** Focus the title field on mount (Save-as-outfit, v4 `:131-134`). */
  readonly autoFocusTitle = input(false);

  readonly closed = output<void>();
  readonly saved = output<void>();

  protected readonly slotTypes = WARDROBE_SLOT_TYPES;

  protected readonly isEditing = computed(() => this.item() !== null);
  /** Whether the item lives in a shared tier rather than a character vault
   *  (v4 `:57-62`): on edit fixed by the item's own tier; on create the
   *  "Add to" selector governs routing and this only seeds the notice. */
  protected readonly isShared = computed(() => {
    const existing = this.item();
    return this.isSharedInput() || (existing !== null && !existing.characterId);
  });

  /** Destination for a NEW item (v4 `:63-68`). */
  protected readonly createScope = signal<WardrobeCreateScope>('character');

  // Form state (v4 useFormState, :70-76).
  protected readonly title = signal('');
  protected readonly description = signal('');
  protected readonly imagePrompt = signal('');
  protected readonly appropriateness = signal('');
  protected readonly isDefault = signal(false);

  protected readonly selectedTypes = signal<WardrobeSlotType[]>([]);
  protected readonly componentItemIds = signal<string[]>([]);
  /** Composite equip behaviour (v4 `:85-92`): `replace: false` = additive
   *  layering; `bundleDesignatedTypes` lets a replace-composite designate
   *  slots beyond its components' union, seeded from the stored types. */
  protected readonly replace = signal(false);
  protected readonly bundleDesignatedTypes = signal<WardrobeSlotType[]>([]);
  /** Editor mode is independent of `componentItemIds` so the user can switch
   *  without immediately mutating the data (v4 `:93-101`). */
  protected readonly editorMode = signal<EditorMode>('single');

  protected readonly candidates = signal<CandidateItem[]>([]);
  protected readonly candidatesLoading = signal(false);
  protected readonly componentSearch = signal('');
  protected readonly expandedGroups = signal<Set<CandidateGroup>>(new Set(GROUP_ORDER));

  // Validation timing: errors only after submit attempt or focus-then-blur
  // (v4 :110-116).
  protected readonly submitAttempted = signal(false);
  protected readonly touchedTitle = signal(false);
  protected readonly touchedTypes = signal(false);
  protected readonly touchedComponents = signal(false);

  protected readonly showKeepResetPrompt = signal(false);
  protected readonly saving = signal(false);
  protected readonly saveError = signal<string | null>(null);

  private readonly titleInput = viewChild<ElementRef<HTMLInputElement>>('titleInput');
  private seededFor: WardrobeItemDto | null | undefined = undefined;
  private candidatesLoadedFor: string | null = null;

  constructor() {
    // Seed the form from the inputs once they resolve (v4 seeds via useState
    // initializers — the component remounts per open, so one seed per item).
    effect(() => {
      const item = this.item();
      const initialComponents = this.initialComponentItemIds();
      const initialMode = this.initialMode();
      if (this.seededFor !== undefined && this.seededFor === item) return;
      this.seededFor = item;
      untracked(() => {
        this.createScope.set(this.isSharedInput() ? 'global' : 'character');
        this.title.set(item?.title ?? '');
        this.description.set(item?.description ?? '');
        this.imagePrompt.set(item?.imagePrompt ?? '');
        this.appropriateness.set(item?.appropriateness ?? '');
        this.isDefault.set(item?.isDefault ?? false);
        this.selectedTypes.set(item?.types ?? []);
        const seedComponents = initialComponents ?? item?.componentItemIds ?? [];
        this.componentItemIds.set(seedComponents);
        this.replace.set(item?.replace ?? false);
        this.bundleDesignatedTypes.set(item?.types ?? []);
        // v4 :96-101 — initialMode wins; else components imply bundle.
        this.editorMode.set(initialMode ?? (seedComponents.length > 0 ? 'bundle' : 'single'));
        if (this.autoFocusTitle()) {
          setTimeout(() => this.titleInput()?.nativeElement.focus());
        }
      });
    });

    // Load candidate items once per (characterId, projectId) — this
    // character's wardrobe + project + shared archetypes (v4 :139-189).
    effect(() => {
      const characterId = this.characterId();
      const projectId = this.projectId();
      const loadKey = `${characterId}|${projectId ?? ''}`;
      if (this.candidatesLoadedFor === loadKey) return;
      this.candidatesLoadedFor = loadKey;
      untracked(() => void this.loadCandidates(characterId, projectId));
    });
  }

  private async loadCandidates(characterId: string, projectId: string | null): Promise<void> {
    this.candidatesLoading.set(true);
    try {
      // The three tier reads, in parallel (v4 :144-147); each fails soft.
      const [personal, project, archetype] = await Promise.all([
        this.core.dispatchData({ type: 'characterWardrobeList', characterId }).catch(() => null),
        projectId
          ? this.core
              .dispatchData({ type: 'projectWardrobeList', projectId })
              .catch(() => null)
          : Promise.resolve(null),
        dispatchWardrobe(this.core, { type: 'wardrobeList' }).catch(() => null),
      ]);

      const collected: CandidateItem[] = [];
      const push = (data: Record<string, unknown> | null, shared: boolean): void => {
        const list = (data?.['wardrobeItems'] as WardrobeItemDto[] | undefined) ?? [];
        for (const w of list) {
          if (collected.some((c) => c.id === w.id)) continue;
          collected.push({
            id: w.id,
            title: w.title,
            types: w.types,
            componentItemIds: Array.isArray(w.componentItemIds) ? w.componentItemIds : [],
            isShared: shared,
          });
        }
      };
      push(personal, false);
      push(project, true);
      push(archetype, true);
      this.candidates.set(collected);
    } finally {
      this.candidatesLoading.set(false);
    }
  }

  /** v4 `:437-443` — the project arm appears only when a project resolved. */
  protected readonly scopeOptions = computed(
    (): Array<{ scope: WardrobeCreateScope; label: string }> => [
      { scope: 'character', label: 'This character' },
      { scope: 'global', label: 'Shared — everywhere' },
      ...(this.projectId()
        ? [{ scope: 'project' as const, label: 'Shared — this project' }]
        : []),
    ],
  );

  /** v4 `:460-466`. */
  protected readonly scopeHint = computed(() => {
    switch (this.createScope()) {
      case 'character':
        return 'Only this character can wear it.';
      case 'project':
        return "Every character in this project's chats can wear it.";
      default:
        return 'Every character, in every chat, can wear it.';
    }
  });

  /**
   * Items the user can pick as components (v4 `:197-209`), excluding this item
   * itself (self-reference is a trivial cycle) and direct parents (which would
   * make a cycle on save — server enforces this anyway).
   */
  protected readonly eligibleCandidates = computed<CandidateItem[]>(() => {
    const item = this.item();
    const excluded = new Set<string>();
    if (item) {
      excluded.add(item.id);
      for (const c of this.candidates()) {
        if (c.componentItemIds.includes(item.id)) excluded.add(c.id);
      }
    }
    const search = this.componentSearch().trim().toLowerCase();
    return this.candidates()
      .filter((c) => !excluded.has(c.id))
      .filter((c) => (search ? c.title.toLowerCase().includes(search) : true));
  });

  /** v4 `:211-219`. */
  protected readonly groupedCandidates = computed(() => {
    const map = new Map<CandidateGroup, CandidateItem[]>();
    for (const g of GROUP_ORDER) map.set(g, []);
    for (const c of this.eligibleCandidates()) {
      map.get(getCandidateGroup(c))?.push(c);
    }
    return map;
  });

  protected readonly isBundle = computed(() => this.editorMode() === 'bundle');

  /** Auto-computed types from components (v4 `:226-230`) — the server runs the
   *  exact same union; mirrored so the UI always shows what's about to save. */
  protected readonly computedTypes = computed<WardrobeSlotType[]>(() => {
    if (this.componentItemIds().length === 0) return [];
    const components = this.candidates().filter((c) => this.componentItemIds().includes(c.id));
    return unionTypes(components);
  });

  /** v4 `:232-241` — bundle coverage = component union, optionally widened by
   *  designated slots for a replace composite; single mode = user-selected. */
  protected readonly effectiveTypes = computed<WardrobeSlotType[]>(() => {
    if (!this.isBundle()) return this.selectedTypes();
    if (!this.replace()) return this.computedTypes();
    return WARDROBE_SLOT_TYPES.filter(
      (s) => this.computedTypes().includes(s) || this.bundleDesignatedTypes().includes(s),
    );
  });

  /** v4 `:243-257`. */
  protected handleTypeToggle(type: WardrobeSlotType): void {
    if (this.isBundle()) {
      // Only a replace composite designates slots, and only beyond the
      // component union — union slots are always covered (locked on).
      if (!this.replace()) return;
      if (this.computedTypes().includes(type)) return;
      this.bundleDesignatedTypes.update((prev) =>
        prev.includes(type) ? prev.filter((t) => t !== type) : [...prev, type],
      );
      return;
    }
    this.selectedTypes.update((prev) =>
      prev.includes(type) ? prev.filter((t) => t !== type) : [...prev, type],
    );
  }

  /** v4 `:259-263`. */
  protected toggleComponent(id: string): void {
    this.componentItemIds.update((prev) =>
      prev.includes(id) ? prev.filter((c) => c !== id) : [...prev, id],
    );
  }

  /** v4 `:265-272`. */
  protected toggleGroup(group: CandidateGroup): void {
    this.expandedGroups.update((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  }

  /** v4 `:274-283` — bundle → single with components needs an explicit
   *  decision about the components and their derived types. */
  protected handleModeChange(next: EditorMode): void {
    if (next === this.editorMode()) return;
    if (next === 'single' && this.componentItemIds().length > 0) {
      this.showKeepResetPrompt.set(true);
      return;
    }
    this.editorMode.set(next);
  }

  /** v4 `:285-291` — drop the components but lock in the computed types. */
  protected handleConfirmKeepTypes(): void {
    this.selectedTypes.set(this.computedTypes());
    this.componentItemIds.set([]);
    this.editorMode.set('single');
    this.showKeepResetPrompt.set(false);
  }

  /** v4 `:293-298`. */
  protected handleConfirmReset(): void {
    this.selectedTypes.set([]);
    this.componentItemIds.set([]);
    this.editorMode.set('single');
    this.showKeepResetPrompt.set(false);
  }

  // Validation flags — surfaced only after submit attempt or field blur
  // (v4 :300-310).
  protected readonly showTitleError = computed(
    () => !this.title().trim() && (this.submitAttempted() || this.touchedTitle()),
  );
  protected readonly showTypesError = computed(
    () =>
      !this.isBundle() &&
      this.selectedTypes().length === 0 &&
      (this.submitAttempted() || this.touchedTypes()),
  );
  protected readonly showComponentsError = computed(
    () =>
      this.isBundle() &&
      this.componentItemIds().length === 0 &&
      (this.submitAttempted() || this.touchedComponents()),
  );

  /** v4 `:312-315`. */
  protected readonly isSaveDisabled = computed(
    () =>
      !this.title().trim() ||
      (!this.isBundle() && this.selectedTypes().length === 0) ||
      (this.isBundle() && this.componentItemIds().length === 0),
  );

  protected readonly selectedComponents = computed(() =>
    this.componentItemIds()
      .map((id) => this.candidates().find((c) => c.id === id))
      .filter((c): c is CandidateItem => Boolean(c)),
  );

  protected appropriatenessCountClass(): string {
    return charCountClass(this.appropriateness().length, 200);
  }
  protected imagePromptCountClass(): string {
    return charCountClass(this.imagePrompt().length, 200);
  }

  /**
   * The save request, routed BY TIER — v4 `:354-369` transcribed exactly:
   *
   *     isEditing && isShared    → PUT  /api/v1/wardrobe/{item.id}
   *     isEditing && !isShared   → PUT  /api/v1/characters/{cid}/wardrobe/{item.id}
   *     createScope==='project'
   *       && projectId           → POST /api/v1/projects/{projectId}/wardrobe
   *     createScope==='global'   → POST /api/v1/wardrobe
   *     otherwise                → POST /api/v1/characters/{cid}/wardrobe
   *
   * Exposed for the unit spec that pins all five branches.
   */
  buildSaveRequest(): Record<string, unknown> {
    const isBundle = this.isBundle();
    const typesToSave = isBundle ? this.effectiveTypes() : this.selectedTypes();
    const componentsToSave = isBundle ? this.componentItemIds() : [];
    // The payload (v4 :342-352) — `replace` is composite-only; leaf items
    // always replace their slots.
    const item: Record<string, unknown> = {
      title: this.title(),
      description: this.description() || null,
      imagePrompt: this.imagePrompt() || null,
      types: typesToSave,
      appropriateness: this.appropriateness() || null,
      isDefault: this.isDefault(),
      componentItemIds: componentsToSave,
      replace: isBundle ? this.replace() : false,
    };

    const existing = this.item();
    const projectId = this.projectId();
    if (this.isEditing() && existing) {
      return this.isShared()
        ? { type: 'wardrobeUpdate', itemId: existing.id, item }
        : {
            type: 'characterWardrobeUpdate',
            characterId: this.characterId(),
            itemId: existing.id,
            item,
          };
    }
    if (this.createScope() === 'project' && projectId) {
      return { type: 'projectWardrobeCreate', projectId, item };
    }
    if (this.createScope() === 'global') {
      return { type: 'wardrobeCreate', item };
    }
    return { type: 'characterWardrobeCreate', characterId: this.characterId(), item };
  }

  /** v4 `:317-386` — the validation gauntlet, then the tier-routed save. */
  protected async handleSave(): Promise<void> {
    this.submitAttempted.set(true);
    if (!this.title().trim()) {
      this.saveError.set('Enter a title');
      return;
    }
    if (!this.isBundle() && this.selectedTypes().length === 0) {
      this.saveError.set('Select at least one type');
      return;
    }
    if (this.isBundle() && this.componentItemIds().length === 0) {
      this.saveError.set('Add at least one component');
      return;
    }
    if (this.isBundle() && this.computedTypes().length === 0) {
      this.saveError.set('Selected components do not cover any slots');
      return;
    }

    this.saving.set(true);
    this.saveError.set(null);
    try {
      await this.core.dispatchData(
        this.buildSaveRequest() as Parameters<CoreClient['dispatchData']>[0],
      );
      this.saved.emit();
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to save wardrobe item');
    } finally {
      this.saving.set(false);
    }
  }
}
