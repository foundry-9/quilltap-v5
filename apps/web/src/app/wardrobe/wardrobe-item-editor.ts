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
import type { WardrobeItemDto, WardrobeOrigin, WardrobeSlotType } from '../core/core-contract';
import { MarkdownField } from '../editor/markdown-field';
import { buildCompositeTypes, isComposite } from './composite-types';
import {
  draftFromItem,
  draftToPayload,
  emptyDraft,
  payloadForEdit,
  validateDraft,
  type WardrobeItemDraft,
  type WardrobeItemPayload,
} from './item-draft';
import { SlotCheckboxGroup } from './slot-ui';
import { GENERAL_CONTAINER, type WardrobeContainer } from './wardrobe-container';
import {
  containerCreateRequest,
  containerUpdateRequest,
  dispatchWardrobe,
  loadCharacterWardrobeItems,
  loadWardrobeContainerItems,
  type WardrobeContainerRequest,
  type WardrobeLaneRequest,
} from './wardrobe.api';
import { GROUP_ORDER, getCandidateGroup } from './item-editor/constants';
import type { CandidateGroup, CandidateItem } from './item-editor/types';
import { WardrobeComponentPicker } from './item-editor/wardrobe-component-picker';
import { WardrobeModeChangePrompt } from './item-editor/wardrobe-mode-change-prompt';
import { WardrobeWearHistorySection } from './item-editor/wear-history-section';
import { WardrobeItemImageSection } from './item-editor/wardrobe-item-image-section';
import { ToastService } from '../ui/toast.service';

type EditorMode = 'single' | 'bundle';

/** Where a newly-created wardrobe item is written
 *  (v4 `wardrobe-item-editor.tsx:23`). */
export type WardrobeCreateScope = 'character' | 'global' | 'project';

/**
 * v4 `lib/wardrobe/wardrobe-container.ts` `containerForListedItem`
 * (`9ecebf986`, `:88-102` at the pin `4a38222e6`) — the container a listed
 * item is addressed through: the `origin` the collection read attached wins;
 * without one a character-owned item lives in its character's vault and
 * anything else is taken for a General archetype.
 *
 * HANDOFF(P4.D285): a private twin of P4.D285's NEW export in
 * `wardrobe-container.ts` (its file); the unifier deletes this twin and
 * imports theirs by name.
 */
function containerForListedItem(item: {
  characterId?: string | null;
  origin?: WardrobeOrigin | null;
}): WardrobeContainer {
  if (item.origin) return { scope: item.origin.scope, id: item.origin.id };
  return item.characterId ? { scope: 'character', id: item.characterId } : GENERAL_CONTAINER;
}

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
 * At the pin `4a38222e6` (`9ecebf986` + `4a38222e6`, P4.D286): the form is
 * one `item-draft` (validated by the routes' own rules, an unchanged folder
 * never re-sent on edit); the candidate pool is the character's whole reach
 * (own vault, groups, project, General) or the pinned container + General,
 * archived items INCLUDED so an outfit whose part was archived keeps that
 * part's slot (bug 190); a composite's slots are `buildCompositeTypes` —
 * widen, never narrow — for additive and replace outfits alike; an edit PUTs
 * the wardrobe the item's origin names; and the Folder field files the item.
 *
 * DIVERGENCES (deliberate, recorded): the parent reloads on success (the
 * `project-wardrobe-manager` precedent); the description markdown editor is
 * the shared `qt-markdown-field` (v4 `MarkdownLexicalEditor`,
 * `minHeight="10rem"` carried, `:679`) whose absorb-once seam keeps the
 * async-loaded content from marking the form dirty.
 */
@Component({
  selector: 'qt-wardrobe-item-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    MarkdownField,
    SlotCheckboxGroup,
    WardrobeComponentPicker,
    WardrobeModeChangePrompt,
    WardrobeWearHistorySection,
    WardrobeItemImageSection,
  ],
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
          <!-- Pinned destination — creating inside a shared container always
               saves into that container; no scope to choose (v4 :487-509). -->
          @if (!isEditing() && sharedContainer(); as shared) {
            <div>
              <span class="qt-label mb-1 block">Add to</span>
              <p class="text-sm text-foreground">
                {{
                  containerLabel() ??
                    (shared.scope === 'general'
                      ? 'Quilltap General'
                      : shared.scope === 'project'
                        ? 'This project'
                        : 'This group')
                }}
              </p>
              <p class="qt-text-xs qt-text-secondary mt-1">
                {{
                  shared.scope === 'general'
                    ? 'Every character, in every chat, can wear it.'
                    : shared.scope === 'project'
                      ? "Every character in this project's chats can wear it."
                      : 'Every character in this group can wear it.'
                }}
              </p>
            </div>
          }

          <!-- Destination scope — only when creating. Editing keeps an item in
               its existing tier (v4 :511-550). -->
          @if (!isEditing() && !sharedContainer()) {
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

          <!-- Picture — directly under Title, the most visible thing in the
               form. Inert in create mode: a fresh item has no id yet (v4
               7c8572869 :638-646). -->
          <qt-wardrobe-item-image-section
            [item]="item()"
            [container]="itemHomeContainer()"
            (imageChanged)="imageChanged.emit()"
          />

          <!-- Single mode: Types checkboxes (v4 :537-569). -->
          @if (!isBundle()) {
            <div>
              <span class="qt-label mb-2 block">Type(s) *</span>
              <qt-slot-checkbox-group
                [value]="selectedTypes()"
                (toggle)="handleTypeToggle($event)"
                (groupBlur)="touchedTypes.set(true)"
              />
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
              <span class="text-sm text-foreground">{{ defaultOutfitLabel() }}</span>
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

          <!-- Wear ledger — read-only, edit mode only (a new item has none;
               v4 3ee3b1342 :793-801). -->
          @if (item(); as editing) {
            @if (itemHomeContainer(); as home) {
              <qt-wardrobe-wear-history-section
                [itemId]="editing.id"
                [container]="home"
                [createdAt]="editing.createdAt"
                [isComposite]="isComposite(editing)"
              />
            }
          }
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
  private readonly toasts = inject(ToastService);

  /** Owning character — null when the editor is opened on a shared container
   *  (v4 `:31-32`). */
  readonly characterId = input.required<string | null>();
  readonly item = input<WardrobeItemDto | null>(null);
  /**
   * ACCEPTED AND IGNORED — v4 `9ecebf986` REMOVED the `isShared` prop: on edit
   * the item's own tier decides, on create the "Add to" selector does.
   * HANDOFF(P4.D285): kept only because the wardrobe dialog (P4.D285's file)
   * still binds `[isShared]="false"`; the unifier deletes this input once
   * that host drops the binding.
   */
  readonly isSharedInput = input(false, { alias: 'isShared' });
  /** Project context (the chat's project) — when present the create-scope
   *  selector offers a "this project" destination (v4 `:34-38`). */
  readonly projectId = input<string | null>(null);
  /**
   * The container the wardrobe dialog is browsing (v4 `:41-48`). When it is a
   * shared container (General / a project / a group), the editor is PINNED to
   * it: creates POST into it, edits PUT back to it, and the character-view
   * "Add to" selector is replaced by a destination note. Character scope (or
   * absent) keeps the classic character-view behaviour.
   */
  readonly container = input<WardrobeContainer | null>(null);
  /** Display name for {@link container}, e.g. the project or group name. */
  readonly containerLabel = input<string | undefined>(undefined);
  /** Pre-populated component IDs (Save-as-outfit from the Outfit Builder). */
  readonly initialComponentItemIds = input<string[] | undefined>(undefined);
  /** Force a starting mode (Save-as-outfit opens in bundle mode). */
  readonly initialMode = input<EditorMode | undefined>(undefined);
  /** Focus the title field on mount (Save-as-outfit, v4 `:131-134`). */
  readonly autoFocusTitle = input(false);
  /** Create only: the folder the browser is showing, where the new item is
   *  filed by default (v4 `4a38222e6` `:65-66`). */
  readonly initialFolder = input('');

  readonly closed = output<void>();
  readonly saved = output<void>();
  /** The item's current picture changed (v4 `onImageChanged`), so the lists
   *  behind the editor can refresh their thumbnails. */
  readonly imageChanged = output<void>();

  protected readonly isComposite = isComposite;

  protected readonly isEditing = computed(() => this.item() !== null);
  /** A non-character container pins the editor to that container (v4 `:75-76`). */
  protected readonly sharedContainer = computed<WardrobeContainer | null>(() => {
    const container = this.container();
    return container && container.scope !== 'character' ? container : null;
  });
  /** Whether the item lives in a shared tier rather than a character vault
   *  (v4 `:94-97`): fixed by the item's own tier on edit; on create the
   *  "Add to" selector (`createScope`) governs routing. */
  protected readonly isShared = computed(() => {
    const existing = this.item();
    return existing !== null && !existing.characterId;
  });

  /**
   * The item's own route when editing — where Update PUTs, where the wear
   * history and the pictures are read. Pinned to a shared container, that
   * container; otherwise the wardrobe the item's origin names (v4 `9ecebf986`
   * `:355-361`: "an edit must never leak a project or group item into
   * Quilltap General"). Null in create mode.
   */
  protected readonly itemHomeContainer = computed<WardrobeContainer | null>(() => {
    const existing = this.item();
    if (!existing) return null;
    return this.sharedContainer() ?? containerForListedItem(existing);
  });

  /**
   * The wardrobe whose folders the Folder field offers: the item's home on
   * edit, the chosen destination on create (v4 `4a38222e6` `:364-375`).
   */
  protected readonly folderContainer = computed<WardrobeContainer | null>(() => {
    if (this.isEditing()) return this.itemHomeContainer();
    const shared = this.sharedContainer();
    if (shared) return shared;
    const projectId = this.projectId();
    if (this.createScope() === 'project' && projectId) return { scope: 'project', id: projectId };
    if (this.createScope() === 'global') return GENERAL_CONTAINER;
    const characterId = this.characterId();
    return characterId ? { scope: 'character', id: characterId } : null;
  });

  /** Destination for a NEW item (v4 `:63-68`). */
  protected readonly createScope = signal<WardrobeCreateScope>('character');

  /**
   * A default garment is put on at the start of every chat by every character
   * that can reach it — so the checkbox's promise depends on where the item is
   * headed. On edit we know only that the item is shared (General vs project
   * isn't recorded on the item itself), so that copy stays deliberately broad.
   */
  protected readonly defaultOutfitLabel = computed(() => {
    if (this.isEditing()) {
      return this.isShared()
        ? 'Worn by default by every character who can reach this item'
        : "Part of this character's default outfit";
    }
    // Pinned to a shared container, the promise is that container's own
    // (v4 `:98-103`) — including the group arm, which the character-view
    // scope selector has no counterpart for.
    const shared = this.sharedContainer();
    if (shared) {
      switch (shared.scope) {
        case 'general':
          return 'Worn by default by every character';
        case 'project':
          return 'Worn by default by every character in this project';
        default:
          return 'Worn by default by every character in this group';
      }
    }
    switch (this.createScope()) {
      case 'global':
        return 'Worn by default by every character';
      case 'project':
        return 'Worn by default by every character in this project';
      default:
        return "Part of this character's default outfit";
    }
  });

  // Form state (v4 useFormState, :70-76).
  protected readonly title = signal('');
  protected readonly description = signal('');
  protected readonly imagePrompt = signal('');
  protected readonly appropriateness = signal('');
  protected readonly isDefault = signal(false);
  /** Folder under `Wardrobe/` (`''` = root) — the item's own on edit,
   *  `initialFolder` on create (v4 `4a38222e6` `:135`). */
  protected readonly folder = signal('');
  /** The draft the form was seeded from (v4 `initialDraft`, `:125`) — an edit
   *  compares its folder against this one's. */
  private initialDraft: WardrobeItemDraft = emptyDraft();

  protected readonly selectedTypes = signal<WardrobeSlotType[]>([]);
  protected readonly componentItemIds = signal<string[]>([]);
  /** Composite equip behaviour (v4 `:139-147`): `replace: false` = additive
   *  layering; `bundleDesignatedTypes` are the slots a composite covers beyond
   *  its components' union — seeded from the stored types, so editing only
   *  ever widens (`buildCompositeTypes`). */
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
        // v4 `:101` — a create always starts in the character scope (the
        // removed `isShared` prop no longer seeds it).
        this.createScope.set('character');
        // The one form-state shape (v4 `:123-147`), seeded from the item or blank.
        const draft = item ? draftFromItem(item) : emptyDraft();
        this.initialDraft = draft;
        this.title.set(draft.title);
        this.description.set(draft.description);
        this.imagePrompt.set(draft.imagePrompt);
        this.appropriateness.set(draft.appropriateness);
        this.isDefault.set(draft.isDefault);
        this.selectedTypes.set(draft.types);
        this.folder.set(item ? draft.folder : this.initialFolder());
        const seedComponents = initialComponents ?? draft.componentItemIds;
        this.componentItemIds.set(seedComponents);
        this.replace.set(draft.replace);
        this.bundleDesignatedTypes.set(draft.types);
        // v4 :96-101 — initialMode wins; else components imply bundle.
        this.editorMode.set(initialMode ?? (seedComponents.length > 0 ? 'bundle' : 'single'));
        if (this.autoFocusTitle()) {
          setTimeout(() => this.titleInput()?.nativeElement.focus());
        }
      });
    });

    // Load candidate items once per (characterId, projectId, container) —
    // through the same tier loaders every wardrobe list uses (v4 `9ecebf986`
    // `:189-225`): in the character view the character's whole reach (own
    // vault, groups, project, General); pinned to a shared container that
    // container + General. Keyed on `container?.scope` / `container?.id` as
    // v4's dep array is.
    effect(() => {
      const characterId = this.characterId();
      const projectId = this.projectId();
      const container = this.container();
      const loadKey = `${characterId ?? ''}|${projectId ?? ''}|${container?.scope ?? ''}|${container?.id ?? ''}`;
      if (this.candidatesLoadedFor === loadKey) return;
      this.candidatesLoadedFor = loadKey;
      untracked(() => void this.loadCandidates(characterId, projectId));
    });
  }

  private async loadCandidates(characterId: string | null, projectId: string | null): Promise<void> {
    const shared = this.sharedContainer();
    this.candidatesLoading.set(true);
    try {
      // Archived items are included so a composite whose parts were archived
      // still resolves (and keeps) their slots (bug 190, v4 `:189-198`).
      let pool: WardrobeItemDto[];
      let localIds: Set<string> | null = null;
      if (shared) {
        const loaded = await loadWardrobeContainerItems(this.core, shared, {
          includeArchived: true,
        });
        pool = loaded.resolutionItems;
        localIds = new Set(loaded.items.map((i) => i.id));
      } else {
        pool = (
          await loadCharacterWardrobeItems(this.core, characterId, {
            projectId,
            includeArchived: true,
          })
        ).items;
      }
      this.candidates.set(
        pool.map((w) => {
          // Items in the wardrobe being edited get no origin chip; borrowed
          // ones keep the origin their read attached (v4 `:207-219`).
          const local = localIds ? localIds.has(w.id) : Boolean(w.characterId);
          return {
            id: w.id,
            title: w.title,
            types: w.types,
            componentItemIds: Array.isArray(w.componentItemIds) ? w.componentItemIds : [],
            origin: local ? null : (w.origin ?? null),
            archived: Boolean(w.archivedAt),
          };
        }),
      );
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
    const componentItemIds = this.componentItemIds();
    return (
      this.candidates()
        .filter((c) => !excluded.has(c.id))
        // An archived item can't be newly bundled, but one already in the
        // outfit stays listed so it can be taken out (v4 `9ecebf986` `:244-246`).
        .filter((c) => !c.archived || componentItemIds.includes(c.id))
        .filter((c) => (search ? c.title.toLowerCase().includes(search) : true))
    );
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

  /** The chosen components this editor can resolve. */
  private readonly resolvedComponents = computed(() =>
    this.candidates().filter((c) => this.componentItemIds().includes(c.id)),
  );

  /** The slots the chosen components cover (union) — locked on in the slot
   *  designation (v4 `9ecebf986` `:262-268`). */
  protected readonly computedTypes = computed<WardrobeSlotType[]>(() => {
    if (this.componentItemIds().length === 0) return [];
    return buildCompositeTypes(this.resolvedComponents());
  });

  /**
   * Bundle coverage = the component union widened by the designated slots —
   * `buildCompositeTypes`, the server's own rule (v4 `:270-280`). It never
   * narrows: a component this editor can't resolve, or one removed, leaves
   * the slots the item already claimed in place until the user unticks them.
   * In single mode, types are always user-selected.
   */
  protected readonly effectiveTypes = computed<WardrobeSlotType[]>(() =>
    this.isBundle()
      ? buildCompositeTypes(this.resolvedComponents(), this.bundleDesignatedTypes())
      : this.selectedTypes(),
  );

  /** v4 `:282-295`. */
  protected handleTypeToggle(type: WardrobeSlotType): void {
    if (this.isBundle()) {
      // Designated slots beyond the component union; union slots are always
      // covered (locked on).
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

  /** v4 `:323-329` — drop the components but lock in the types they had
   *  been computing (the EFFECTIVE ones, designated slots included). */
  protected handleConfirmKeepTypes(): void {
    this.selectedTypes.set(this.effectiveTypes());
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

  /**
   * Every component keeps a chip — one this editor can't resolve (a store it
   * can't see) shows as such rather than vanishing, and still saves (v4
   * `9ecebf986` `:437-453`). While the candidates load, only the resolvable.
   */
  protected readonly selectedComponents = computed<CandidateItem[]>(() => {
    const candidates = this.candidates();
    const ids = this.componentItemIds();
    if (this.candidatesLoading()) return candidates.filter((c) => ids.includes(c.id));
    return ids.map(
      (id) =>
        candidates.find((c) => c.id === id) ?? {
          id,
          title: 'A garment beyond this wardrobe’s reach',
          types: [],
          componentItemIds: [],
          origin: null,
        },
    );
  });

  protected appropriatenessCountClass(): string {
    return charCountClass(this.appropriateness().length, 200);
  }
  protected imagePromptCountClass(): string {
    return charCountClass(this.imagePrompt().length, 200);
  }

  /** The form as a draft (v4 `:384-390`). */
  private currentDraft(): WardrobeItemDraft {
    const isBundle = this.isBundle();
    return {
      title: this.title(),
      description: this.description(),
      imagePrompt: this.imagePrompt(),
      appropriateness: this.appropriateness(),
      isDefault: this.isDefault(),
      types: isBundle ? this.effectiveTypes() : this.selectedTypes(),
      componentItemIds: isBundle ? this.componentItemIds() : [],
      replace: this.replace(),
      folder: this.folder(),
    };
  }

  /**
   * The body for this save: `draftToPayload`, and on an edit without the
   * folder when it is unchanged (v4 `:396` — "An edit that leaves the folder
   * alone doesn't send it").
   */
  private payloadFor(payload: WardrobeItemPayload): Record<string, unknown> {
    const body = this.isEditing() ? payloadForEdit(payload, this.initialDraft.folder) : payload;
    return { ...body };
  }

  /**
   * The save request, routed BY CONTAINER then BY TIER — v4 `9ecebf986`
   * `:400-417`:
   *
   *     isEditing                     → PUT  wardrobeItemUrl(itemHomeContainer, id)
   *       (itemHomeContainer = sharedContainer ?? containerForListedItem(item))
   *     sharedContainer && !isEditing → POST wardrobeCollectionUrl(container)
   *     createScope==='project'
   *       && projectId                → POST /api/v1/projects/{projectId}/wardrobe
   *     createScope==='global'        → POST /api/v1/wardrobe
   *     otherwise                     → POST /api/v1/characters/{cid}/wardrobe
   *
   * The shared-container arm is `d7263f39`'s bug fix, and **v5 measurably had
   * the bug**: before P4.D35 an edit to any shared item PUT `wardrobeUpdate`
   * — Quilltap General — no matter which store the item actually lived in.
   * `9ecebf986` closes the same hole from the character view: a project- or
   * group-origin item there PUTs its origin's route, where v5's old
   * `isShared → General` rule forked a General copy.
   *
   * Exposed for the unit spec that pins every branch.
   */
  buildSaveRequest(
    payload: WardrobeItemPayload = draftToPayload(this.currentDraft()),
  ): WardrobeLaneRequest | WardrobeContainerRequest {
    const item = this.payloadFor(payload);
    const existing = this.item();
    const home = this.itemHomeContainer();
    if (existing && home) return containerUpdateRequest(home, existing.id, item);
    const shared = this.sharedContainer();
    if (shared) return containerCreateRequest(shared, item);
    const projectId = this.projectId();
    if (this.createScope() === 'project' && projectId) {
      return { type: 'projectWardrobeCreate', projectId, item };
    }
    const characterId = this.characterId();
    if (this.createScope() === 'global' || !characterId) {
      return { type: 'wardrobeCreate', item };
    }
    return { type: 'characterWardrobeCreate', characterId, item };
  }

  /** v4 `:377-435` — one guard, the routes' own validation, the routed save. */
  protected async handleSave(): Promise<void> {
    this.submitAttempted.set(true);
    if (this.isBundle() && this.componentItemIds().length === 0) {
      this.toasts.showError('Add at least one component');
      return;
    }
    // One validation, the routes' own body schema (title, at least one slot).
    const validation = validateDraft(this.currentDraft());
    if (!validation.ok) {
      this.toasts.showError(validation.error);
      return;
    }

    this.saving.set(true);
    try {
      await dispatchWardrobe(this.core, this.buildSaveRequest(validation.payload));
      // v4 `:432` — the success toast, before onSave.
      this.toasts.showSuccess(this.isEditing() ? 'Wardrobe item updated' : 'Wardrobe item created');
      this.saved.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to save wardrobe item');
    } finally {
      this.saving.set(false);
    }
  }
}
