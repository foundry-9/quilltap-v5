import { NgTemplateOutlet } from '@angular/common';
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
} from '@angular/core';

import { apiUrl } from '../core/api-url';
import { CoreClient } from '../core/core-client';
import type { CharacterListItem, WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { characterAvatarSrc } from '../screens/characters/characters.api';
import { AvatarGenerationPane, type ImageProfileSummary } from './avatar-generation-pane';
import {
  buildDefaultOutfit,
  cloneSlots,
  EMPTY_EQUIPPED_SLOTS,
  freshSlots,
  nextCopyTitle,
  takeOffBundleFromSlots,
  breakApartBundleInSlots,
  wearItemIntoSlots,
  type EquippedBundle,
  type EquippedSlots,
} from './equipped-slots';
import { createOutfitStore, type OutfitStore } from './outfit-store';
import { equippedSlotsEqual } from './staged-live-outfits';
import { OutfitComposer } from './outfit-composer';
import { WardrobeDialogService } from './wardrobe-dialog.service';
import { WardrobeItemEditor } from './wardrobe-item-editor';
import { WardrobeItemRow } from './wardrobe-item-row';
import { WardrobeTransferDialog, type TransferMode } from './wardrobe-transfer-dialog';
import {
  deleteWardrobeItem,
  dispatchWardrobe,
  duplicateWardrobeItem,
  loadCharacterWardrobeItems,
  toggleItemDefault,
} from './wardrobe.api';
import { ToastService } from '../ui/toast.service';

interface CharacterSummary {
  id: string;
  name: string;
  avatarUrl?: string | null;
}

type SlotFilter = 'all' | WardrobeSlotType;
const SLOT_FILTERS: SlotFilter[] = ['all', 'top', 'bottom', 'footwear', 'accessories'];
type ItemKind = 'items' | 'outfits';
type RightTab = 'live' | 'builder';
type EditorIntent = 'create-single' | 'create-bundle';

/**
 * The inner wardrobe dialog body — a port of v4
 * `components/wardrobe/wardrobe-control-dialog.tsx` `WardrobeControlDialogInner`
 * (`:166`). Remounted by the host per `${characterId}|${chatId}` context (v4's
 * `key`, `:92`), which discards ALL staged state on a context change — that
 * remount is load-bearing; do not convert it into an in-place update.
 *
 * THE CHAT-AWARE BEHAVIOR (v4 `:203-204`, `:227-236`, `:730`):
 *  - **In chat** (`chatId !== null`) a "Live outfit" tab exists and every live
 *    mutation STAGES client-side into `liveStagedByChar` — the staging
 *    handlers are pure state updates that fire NO route. On Done,
 *    `flushStagedLiveOutfits` fires ONE `set_all` per dirty character, which
 *    is what keeps it to one Aurora announcement and one avatar regeneration
 *    each. "Try on" → the same `set_all` shape, then closes.
 *  - **Out of chat** there is no live tab and NO equip route EVER fires:
 *    `useFittingActions` is forced true, rows mutate only the transient
 *    `fittingSlots`, and avatar generation goes to the non-persisting
 *    `preview-avatar`.
 *
 * DIVERGENCES (deliberate, recorded): v4's imperative `showConfirmation` is
 * `window.confirm` (the character-edit precedent) — being synchronous and
 * native-modal it also collapses v4's `confirming` click-outside suspension
 * (`:213-225`). The "Import from image" button
 * is NOT shipped (tier 3 — its `analyze-image` verb is refusal-armed in lane
 * P4.9f1 per §4; a dead button is worse than an absent one).
 *
 * THE `asTab` CHROME SWITCH (v4 `WardrobeShell`, `:128-164`; p4.9j3): with
 * `asTab` true the body renders BARE in a `qt-wardrobe-tab` scroll container —
 * no overlay, no title header/close, no footer, no Escape/backdrop close (v4
 * omits `BaseModal` entirely). The `WardrobeView` workspace tab passes it with
 * `chatId=null`, so `isInChat` is false ⇒ no Live-outfit tab and NO equip route
 * ever fires — Outfit Builder only. `asTab` false is the floating dialog above.
 */
@Component({
  selector: 'qt-wardrobe-dialog-inner',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    NgTemplateOutlet,
    AvatarGenerationPane,
    OutfitComposer,
    WardrobeItemEditor,
    WardrobeItemRow,
    WardrobeTransferDialog,
  ],
  host: {
    '(document:keydown.escape)': 'onEscape($event)',
    '(document:mousedown)': 'onDocumentMouseDown()',
  },
  template: `
    <!-- The chrome switch (v4 WardrobeShell, :128-164). asTab renders bare in a
         scroll container (no overlay/header/footer, no Escape/backdrop close);
         otherwise the floating BaseModal (title "Wardrobe", maxWidth 4xl, footer
         Done). Backdrop/Escape close is suspended while a stacked editor/transfer
         dialog is up (v4 closeOnClickOutside). The shared body lives in the
         #body ng-template below. -->
    @if (asTab()) {
      <div class="qt-wardrobe-tab flex flex-col h-full min-h-0 overflow-y-auto p-4">
        <ng-container [ngTemplateOutlet]="body" />
      </div>
    } @else {
      <div class="qt-dialog-overlay" (click)="onBackdrop()">
        <div
          class="qt-dialog flex flex-col max-h-[90vh]"
          style="max-width: 56rem"
          role="dialog"
          aria-modal="true"
          (click)="$event.stopPropagation()"
        >
          <div class="qt-dialog-header flex items-center justify-between gap-4">
            <h2 class="qt-dialog-title">Wardrobe</h2>
            <button
              type="button"
              class="qt-button-ghost qt-button-sm"
              aria-label="Close"
              (click)="requestClose()"
            >
              ✕
            </button>
          </div>

          <div class="qt-dialog-body overflow-y-auto">
            <ng-container [ngTemplateOutlet]="body" />
          </div>

          <div class="qt-dialog-footer">
            <div class="flex items-center justify-end gap-2 w-full">
              <button
                type="button"
                (click)="requestClose()"
                class="qt-button-secondary qt-button-sm"
              >
                Done
              </button>
            </div>
          </div>
        </div>
      </div>
    }

    <!-- The shared dialog body (v4 WardrobeShell children): the flush notice,
         character selector, and the wardrobe/builder grid. -->
    <ng-template #body>
          <!-- Character selector (v4 :887-918) -->
          <div class="flex flex-col gap-3 mb-3">
            <div class="flex items-center gap-2">
              <label for="wardrobe-char-select" class="qt-text-sm qt-text-secondary">
                Character:
              </label>
              @if (selectedCharacter()?.avatarUrl; as avatarUrl) {
                <img
                  [src]="avatarUrl"
                  alt=""
                  class="w-6 h-6 rounded-full object-cover qt-bg-muted border qt-border-default flex-shrink-0"
                />
              }
              <!-- Async options → [selected] per option (dogfood-#6). -->
              <select
                id="wardrobe-char-select"
                class="qt-select flex-1 max-w-md"
                (change)="selectedCharacterId.set($any($event.target).value || null)"
              >
                @if (characters().length === 0) {
                  <option value="" disabled [selected]="!selectedCharacterId()">
                    No characters available
                  </option>
                }
                @for (c of characters(); track c.id) {
                  <option [value]="c.id" [selected]="c.id === selectedCharacterId()">
                    {{ c.name }}
                  </option>
                }
              </select>
            </div>
          </div>

          <div class="grid md:grid-cols-2 gap-4">
            <!-- LEFT: Wardrobe list (v4 :920-1032) -->
            <section class="flex flex-col min-h-0 relative">
              <div class="flex flex-col gap-2 mb-2">
                <input
                  type="search"
                  [value]="titleFilter()"
                  (input)="titleFilter.set($any($event.target).value)"
                  placeholder="Search wardrobe…"
                  class="qt-input qt-input-sm"
                  aria-label="Search wardrobe by title"
                />
                <div
                  role="tablist"
                  aria-label="Item kind"
                  class="inline-flex gap-1 qt-bg-muted/50 rounded-lg p-1 self-start"
                >
                  @for (k of itemKinds; track k) {
                    <button
                      type="button"
                      role="tab"
                      [attr.aria-selected]="kindFilter() === k"
                      (click)="kindFilter.set(k)"
                      class="px-3 py-1 rounded text-xs font-medium transition-colors"
                      [class.qt-bg-default]="kindFilter() === k"
                      [class.text-foreground]="kindFilter() === k"
                      [class.shadow-sm]="kindFilter() === k"
                      [class.qt-text-secondary]="kindFilter() !== k"
                    >
                      {{ k === 'items' ? 'Items' : 'Outfits' }}
                    </button>
                  }
                </div>
                <div class="flex flex-wrap gap-1">
                  @for (slot of slotFilters; track slot) {
                    <button
                      type="button"
                      (click)="slotFilter.set(slot)"
                      class="qt-button-sm"
                      [class.qt-button-secondary]="slotFilter() === slot"
                      [class.qt-button-ghost]="slotFilter() !== slot"
                    >
                      {{ slot === 'all' ? 'All' : slot[0].toUpperCase() + slot.slice(1) }}
                    </button>
                  }
                </div>
              </div>

              <div class="flex-1 overflow-y-auto space-y-1 max-h-[55vh] pb-12">
                @if (!selectedCharacterId()) {
                  <div class="qt-text-sm qt-text-secondary px-3 py-4">
                    Select a character to see their wardrobe.
                  </div>
                } @else if (itemsLoading()) {
                  <div class="qt-text-sm qt-text-secondary px-3 py-4">Loading…</div>
                } @else if (filteredItems().length === 0) {
                  <div class="qt-text-sm qt-text-secondary px-3 py-4">
                    No items match this filter.
                  </div>
                } @else {
                  @for (item of filteredItems(); track item.id) {
                    <!-- inChat gates the row's Wear/+Layer visibility; we want
                         them visible whenever the dialog shows equip-able state
                         — that's any time (v4 :985-989). -->
                    <qt-wardrobe-item-row
                      [item]="item"
                      [allItems]="items()"
                      [inChat]="true"
                      [equipLabel]="useFittingActions() ? 'Try on' : 'Wear'"
                      [addAction]="useFittingActions() ? 'add' : 'layer'"
                      [isUpdatingDefault]="updatingDefaultId() === item.id"
                      (toggleDefault)="handleToggleDefault($event)"
                      (edit)="editingItem.set($event)"
                      (duplicate)="handleDuplicate($event)"
                      (move)="openTransfer($event, 'move')"
                      (copyItem)="openTransfer($event, 'copy')"
                      (delete)="handleDelete($event)"
                      (equip)="rowEquip($event)"
                      (addToSlot)="rowAddToSlot($event.item, $event.slot)"
                    />
                  }
                }
              </div>

              <!-- Sticky create controls (v4 :1013-1031; the Import-from-image
                   entry is a tier-3 deferral — no dead button). -->
              <div
                class="sticky bottom-0 -mx-1 px-1 pt-2 pb-1 qt-bg-default border-t qt-border-default flex items-center gap-2 justify-end"
              >
                <button
                  type="button"
                  class="qt-button-primary qt-button-sm"
                  [disabled]="!selectedCharacterId()"
                  (click)="creatingNew.set('create-single')"
                >
                  + New Item
                </button>
              </div>
            </section>

            <!-- RIGHT: Live outfit / Outfit Builder — Builder always present;
                 Live outfit only when there's a chat to mutate against. -->
            @if (selectedCharacterId()) {
              <section class="flex flex-col min-h-0">
                <div class="flex items-center gap-1 mb-2 qt-tab-group">
                  @if (isInChat) {
                    <button
                      type="button"
                      (click)="rightTab.set('live')"
                      class="qt-tab"
                      [class.qt-tab-active]="rightTab() === 'live'"
                    >
                      Live outfit
                      @if (selectedCharacter(); as sc) {
                        <span class="qt-text-xs qt-text-secondary ml-1">
                          · {{ sc.name }} in this chat
                        </span>
                      }
                    </button>
                  }
                  <button
                    type="button"
                    (click)="rightTab.set('builder')"
                    class="qt-tab"
                    [class.qt-tab-active]="rightTab() === 'builder'"
                  >
                    Outfit Builder
                  </button>
                </div>

                @if (rightTab() === 'live' && isInChat) {
                  <div class="space-y-2 mb-3">
                    <p class="qt-text-xs qt-text-secondary px-1">
                      Edits stage here and apply when you click Done. Nothing happens until then.
                    </p>
                    <qt-outfit-composer
                      [items]="items()"
                      [slots]="liveDisplaySlots()"
                      [showBundleActions]="true"
                      (addToSlot)="handleSlotAdd($event.slot, $event.itemId)"
                      (removeFromSlot)="handleSlotRemove($event.slot, $event.itemId)"
                      (clearSlot)="handleSlotClear($event)"
                      (takeOffBundle)="handleLiveTakeOffBundle($event)"
                      (breakApartBundle)="handleLiveBreakApartBundle($event)"
                    />
                  </div>
                } @else {
                  <div class="space-y-2 mb-3">
                    <p class="qt-text-xs qt-text-secondary px-1">
                      Compose an outfit. Save it as a reusable bundle, try it on, or generate a
                      preview avatar.
                    </p>
                    <div class="flex flex-wrap gap-1 px-1 items-center">
                      <button
                        type="button"
                        (click)="handleSaveAsOutfit()"
                        class="qt-button-primary qt-button-sm"
                        title="Save this composition as a new outfit bundle"
                      >
                        Save as outfit
                      </button>
                      @if (isInChat) {
                        <button
                          type="button"
                          (click)="wearFitting()"
                          class="qt-button-secondary qt-button-sm"
                          title="Replace what the character is wearing with this composition"
                        >
                          Try on
                        </button>
                      }
                      <div class="relative" (mousedown)="$event.stopPropagation()">
                        <button
                          type="button"
                          (click)="resetMenuOpen.set(!resetMenuOpen())"
                          class="qt-button-ghost qt-button-sm"
                          aria-haspopup="menu"
                          [attr.aria-expanded]="resetMenuOpen()"
                          title="Reset the staged composition"
                        >
                          Reset…
                        </button>
                        @if (resetMenuOpen()) {
                          <div
                            role="menu"
                            class="absolute left-0 top-full mt-1 z-30 min-w-[14rem] rounded border qt-border-default qt-bg-default shadow-md"
                          >
                            <ul class="divide-y qt-border-default">
                              @if (isInChat) {
                                <li>
                                  <button
                                    type="button"
                                    role="menuitem"
                                    (click)="resetMenuOpen.set(false); fittingResetToWorn()"
                                    class="block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted"
                                  >
                                    Reset to worn
                                  </button>
                                </li>
                              }
                              <li>
                                <button
                                  type="button"
                                  role="menuitem"
                                  (click)="resetMenuOpen.set(false); fittingResetToDefaults()"
                                  class="block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted"
                                >
                                  Reset to defaults
                                </button>
                              </li>
                              <li>
                                <button
                                  type="button"
                                  role="menuitem"
                                  (click)="resetMenuOpen.set(false); fittingClearAll()"
                                  class="block w-full text-left px-3 py-2 text-sm qt-text-secondary hover:qt-bg-muted"
                                >
                                  Clear all
                                </button>
                              </li>
                            </ul>
                          </div>
                        }
                      </div>
                    </div>
                    <qt-outfit-composer
                      [items]="items()"
                      [slots]="fittingSlots()"
                      [showBundleActions]="true"
                      (addToSlot)="fittingAdd($event.slot, $event.itemId)"
                      (removeFromSlot)="fittingRemove($event.slot, $event.itemId)"
                      (clearSlot)="fittingClear($event)"
                      (takeOffBundle)="handleFittingTakeOffBundle($event)"
                      (breakApartBundle)="handleFittingBreakApartBundle($event)"
                    />
                  </div>
                }

                @if (rightTab() === 'builder') {
                  <qt-avatar-generation-pane
                    [selectedCharacterName]="selectedCharacter()?.name ?? ''"
                    [imageProfiles]="imageProfiles()"
                    [selectedImageProfileId]="selectedImageProfileId()"
                    [generating]="generating()"
                    [inChat]="isInChat"
                    [previewUrl]="isInChat ? null : previewUrl()"
                    [previewFilename]="isInChat ? null : previewFilename()"
                    (selectImageProfile)="selectedImageProfileId.set($event)"
                    (generate)="handleGenerateAvatar()"
                    (discardPreview)="previewUrl.set(null); previewFilename.set(null)"
                  />
                }
              </section>
            }
          </div>
    </ng-template>

    <!-- Inline editor — stacked on top of the dialog (v4 :1213-1246). -->
    @if ((editingItem() || creatingNew()) && selectedCharacterId(); as cid) {
      <qt-wardrobe-item-editor
        [characterId]="cid"
        [item]="editingItem()"
        [isShared]="false"
        [projectId]="dialogProjectId()"
        [initialMode]="editorInitialMode()"
        [initialComponentItemIds]="
          creatingNew() === 'create-bundle' ? createBundleComponents() : undefined
        "
        [autoFocusTitle]="creatingNew() === 'create-bundle'"
        (closed)="closeEditor()"
        (saved)="onEditorSaved()"
      />
    }

    <!-- Transfer dialog — stacked on top of the dialog (v4 :1248-1269). -->
    @if (transferringItem(); as item) {
      @if (transferIntent(); as intent) {
        @if (selectedCharacterId(); as cid) {
          <qt-wardrobe-transfer-dialog
            [mode]="intent"
            [item]="item"
            [sourceCharacterId]="cid"
            [sourceProjectId]="dialogProjectId()"
            (closed)="closeTransfer()"
            (transferred)="onTransferred()"
          />
        }
      }
    }
  `,
})
export class WardrobeControlDialogInner {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly initialCharacterId = input.required<string | null>();
  readonly chatId = input.required<string | null>();
  /**
   * v4 `WardrobeShell` `asTab` (`:143`): render bare for a workspace tab
   * instead of inside the floating modal. Fixed per mount.
   */
  readonly asTab = input<boolean>(false);
  readonly closed = output<void>();

  protected readonly itemKinds: ItemKind[] = ['items', 'outfits'];
  protected readonly slotFilters = SLOT_FILTERS;

  protected readonly characters = signal<CharacterSummary[]>([]);
  protected readonly selectedCharacterId = signal<string | null>(null);
  protected readonly items = signal<WardrobeItemDto[]>([]);
  protected readonly itemsLoading = signal(false);
  protected readonly dialogProjectId = signal<string | null>(null);

  protected readonly editingItem = signal<WardrobeItemDto | null>(null);
  protected readonly transferringItem = signal<WardrobeItemDto | null>(null);
  protected readonly transferIntent = signal<TransferMode | null>(null);
  /** null = no editor open; 'create-single' / 'create-bundle' = new item. */
  protected readonly creatingNew = signal<EditorIntent | null>(null);
  /** Pre-populated component ids when Save-as-outfit opens the editor. */
  protected readonly createBundleComponents = signal<string[]>([]);
  protected readonly slotFilter = signal<SlotFilter>('all');
  protected readonly kindFilter = signal<ItemKind>('items');
  protected readonly titleFilter = signal('');
  protected readonly updatingDefaultId = signal<string | null>(null);

  protected readonly imageProfiles = signal<ImageProfileSummary[]>([]);
  protected readonly selectedImageProfileId = signal<string | null>(null);
  protected readonly generating = signal(false);
  protected readonly previewUrl = signal<string | null>(null);
  protected readonly previewFilename = signal<string | null>(null);

  /** Fitting room — transient; never hits the equip API (v4 :196-202). */
  protected readonly fittingSlots = signal<EquippedSlots>(freshSlots());
  /** v4 `:203` — the load-bearing chat-awareness flag. Fixed per mount (the
   *  remount key includes chatId). */
  protected isInChat = false;
  protected readonly rightTab = signal<RightTab>('builder');
  protected readonly resetMenuOpen = signal(false);
  private fittingSeedKey: string | null = null;

  // -------- Staged Live-outfit edits (v4 :227-238) --------
  //
  // Per-slot tweaks on the Live tab used to call the equip API one at a time,
  // and the server fired a fresh avatar regen + Aurora announcement on every
  // call. Now: every Live-tab mutation stages here, keyed by character. On
  // Done, each character whose staged slots differ from their baseline gets
  // one `set_all` — so at most one announcement and one regen per character
  // per dialog session.
  protected readonly liveStagedByChar = signal<Record<string, EquippedSlots>>({});
  private readonly liveBaselineByChar: Record<string, EquippedSlots> = {};
  private readonly liveSeededByChar = new Set<string>();

  protected outfit!: OutfitStore;
  private booted = false;

  constructor() {
    // One-time boot once the inputs resolve (the component remounts per
    // context key, so this runs exactly once per context).
    effect(() => {
      const initialCharacterId = this.initialCharacterId();
      const chatId = this.chatId();
      if (this.booted) return;
      this.booted = true;
      untracked(() => {
        this.isInChat = chatId !== null;
        this.rightTab.set(this.isInChat ? 'live' : 'builder');
        this.selectedCharacterId.set(initialCharacterId);
        this.outfit = createOutfitStore(this.core, chatId, () => {
          const id = this.selectedCharacterId();
          return id ? [id] : [];
        });
        void this.loadCharacters();
        void this.loadImageProfiles();
        void this.outfit.refreshOutfit();
      });
    });

    // Reload the three-tier items whenever the selected character changes
    // (v4 use-character-wardrobe-items refires on its deps).
    effect(() => {
      const characterId = this.selectedCharacterId();
      untracked(() => void this.reloadItems(characterId));
    });

    // Fitting-room seeding (v4 :299-320): once per character. In chat wait
    // for the worn snapshot AND items; out of chat wait for items so the
    // defaults can be picked out.
    effect(() => {
      const characterId = this.selectedCharacterId();
      const items = this.items();
      const outfitState = this.booted ? this.outfit.outfitState() : {};
      if (!characterId) return;
      const wornSlots = this.isInChat ? outfitState[characterId]?.slots : undefined;
      if (this.isInChat && !wornSlots) return;
      if (items.length === 0 && !wornSlots) return;

      const seedKey = `${characterId}|${this.chatId() ?? 'no-chat'}`;
      if (this.fittingSeedKey === seedKey) return;
      this.fittingSeedKey = seedKey;

      const seed = this.isInChat && wornSlots ? cloneSlots(wornSlots) : buildDefaultOutfit(items);
      untracked(() => this.fittingSlots.set(seed));
    });

    // Seed staged Live slots once a worn snapshot exists for this character
    // (v4 :322-335). The baseline is captured separately so the Done flush can
    // skip no-op commits; the seeded-set gates re-seeding so refreshOutfit
    // round-trips don't blow away in-progress edits.
    effect(() => {
      const characterId = this.selectedCharacterId();
      const outfitState = this.booted ? this.outfit.outfitState() : {};
      if (!this.isInChat || !characterId) return;
      const wornSlots = outfitState[characterId]?.slots;
      if (!wornSlots) return;
      const seedKey = `${characterId}|${this.chatId() ?? 'no-chat'}`;
      if (this.liveSeededByChar.has(seedKey)) return;
      this.liveSeededByChar.add(seedKey);
      this.liveBaselineByChar[characterId] = cloneSlots(wornSlots);
      untracked(() =>
        this.liveStagedByChar.update((prev) => ({
          ...prev,
          [characterId]: cloneSlots(wornSlots),
        })),
      );
    });
  }

  // ---------------------------------------------------------------------------
  // Loads
  // ---------------------------------------------------------------------------

  /** v4 `:249-270` — load characters once; sort by name; auto-select first. */
  private async loadCharacters(): Promise<void> {
    try {
      const data = await this.core.dispatchData({ type: 'characterList' });
      const list = (data['characters'] as CharacterListItem[]) ?? [];
      const sorted = list
        .map(
          (c): CharacterSummary => ({
            id: c.id,
            name: c.name,
            avatarUrl: characterAvatarSrc(c.defaultImage, c.defaultImageId),
          }),
        )
        .sort((a, b) => a.name.localeCompare(b.name));
      this.characters.set(sorted);
      // Auto-select if nothing was specified (v4 :260).
      if (!this.selectedCharacterId()) {
        this.selectedCharacterId.set(sorted[0]?.id ?? null);
      }
    } catch {
      this.characters.set([]);
    }
  }

  /** v4 `:275-296` — load image profiles; preselect the system default. */
  private async loadImageProfiles(): Promise<void> {
    try {
      const data = await this.core.dispatchData({ type: 'imageProfileList' });
      const profiles = ((data['profiles'] as ImageProfileSummary[]) ?? []).map((p) => ({
        id: p.id,
        name: p.name,
        provider: p.provider,
        modelName: p.modelName,
        isDefault: p.isDefault,
      }));
      this.imageProfiles.set(profiles);
      const def = profiles.find((p) => p.isDefault) ?? profiles[0];
      if (!this.selectedImageProfileId()) {
        this.selectedImageProfileId.set(def?.id ?? null);
      }
    } catch {
      /* the pane shows its empty option */
    }
  }

  private async reloadItems(characterId: string | null): Promise<void> {
    if (!characterId) {
      this.items.set([]);
      return;
    }
    this.itemsLoading.set(true);
    try {
      const result = await loadCharacterWardrobeItems(this.core, characterId, {
        chatId: this.chatId(),
      });
      this.items.set(result.items);
      this.dialogProjectId.set(result.projectId);
    } finally {
      this.itemsLoading.set(false);
    }
  }

  private async reloadCurrentItems(): Promise<void> {
    await this.reloadItems(this.selectedCharacterId());
  }

  // ---------------------------------------------------------------------------
  // Filtered items (v4 :340-352)
  // ---------------------------------------------------------------------------
  protected readonly filteredItems = computed(() => {
    const sorted = [...this.items()].sort((a, b) => a.title.localeCompare(b.title));
    const term = this.titleFilter().trim().toLowerCase();
    const kindFilter = this.kindFilter();
    const slotFilter = this.slotFilter();
    return sorted.filter((i) => {
      if (i.archivedAt) return false;
      const isComposite = (i.componentItemIds ?? []).length > 0;
      if (kindFilter === 'items' && isComposite) return false;
      if (kindFilter === 'outfits' && !isComposite) return false;
      if (slotFilter !== 'all' && !i.types.includes(slotFilter)) return false;
      if (term && !i.title.toLowerCase().includes(term)) return false;
      return true;
    });
  });

  protected readonly selectedCharacter = computed(
    () => this.characters().find((c) => c.id === this.selectedCharacterId()) ?? null,
  );

  private readonly itemsById = computed(() => new Map(this.items().map((i) => [i.id, i])));

  /** While the editor/transfer modal is up, backdrop/Escape must not close the
   *  outer dialog (v4 `editorOpen`, `:841-843`). */
  protected readonly editorOpen = computed(() =>
    Boolean(
      this.editingItem() ||
        this.creatingNew() ||
        (this.transferringItem() && this.transferIntent()),
    ),
  );

  // ---------------------------------------------------------------------------
  // Item action handlers (v4 :357-448)
  // ---------------------------------------------------------------------------

  protected async handleToggleDefault(item: WardrobeItemDto): Promise<void> {
    if (!this.selectedCharacterId()) return;
    this.updatingDefaultId.set(item.id);
    try {
      await toggleItemDefault(this.core, item);
      await this.reloadCurrentItems();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to update item');
    } finally {
      this.updatingDefaultId.set(null);
    }
  }

  protected async handleDelete(item: WardrobeItemDto): Promise<void> {
    const characterId = this.selectedCharacterId();
    if (!characterId) return;
    if (!window.confirm(`Delete "${item.title}"? This cannot be undone.`)) return;
    try {
      await deleteWardrobeItem(this.core, item);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to delete item');
      return;
    }
    this.toasts.showSuccess(`Deleted "${item.title}"`);
    await this.reloadCurrentItems();
    if (this.isInChat) {
      this.outfit.invalidateWardrobe(characterId);
      await this.outfit.refreshOutfit();
    }
  }

  protected async handleDuplicate(item: WardrobeItemDto): Promise<void> {
    const characterId = this.selectedCharacterId();
    if (!characterId) return;
    const title = nextCopyTitle(
      item.title,
      this.items().map((i) => i.title),
    );
    try {
      await duplicateWardrobeItem(this.core, characterId, item, title);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to duplicate item');
      return;
    }
    this.toasts.showSuccess(`Duplicated "${item.title}"`);
    await this.reloadCurrentItems();
    if (this.isInChat) {
      this.outfit.invalidateWardrobe(characterId);
      await this.outfit.refreshOutfit();
    }
  }

  // -------- Live-tab staging mutators (v4 :455-528) --------
  //
  // Every Live-tab gesture goes through `updateLiveStaged`, which mutates the
  // staged slots for the current character. None of these touch the server.
  private updateLiveStaged(mutator: (prev: EquippedSlots) => EquippedSlots): void {
    const characterId = this.selectedCharacterId();
    if (!characterId) return;
    this.liveStagedByChar.update((prev) => {
      const wornFallback = this.outfit.outfitState()[characterId]?.slots;
      const current =
        prev[characterId] ??
        (wornFallback ? cloneSlots(wornFallback) : cloneSlots(EMPTY_EQUIPPED_SLOTS));
      return { ...prev, [characterId]: mutator(current) };
    });
  }

  /** v4 `:476-482` — "Wear" honors the item's `replace` flag across every slot
   *  it covers. Pure state; fires no route. */
  protected handleEquipItem(item: WardrobeItemDto): void {
    if (!this.isInChat) return;
    this.updateLiveStaged((prev) => wearItemIntoSlots(prev, item));
  }

  /** v4 `:484-494` — pure state; fires no route. */
  protected handleAddToSlot(item: WardrobeItemDto, slot: WardrobeSlotType): void {
    if (!this.isInChat) return;
    if (!item.types.includes(slot)) return;
    this.updateLiveStaged((prev) => {
      if (prev[slot].includes(item.id)) return prev;
      return { ...prev, [slot]: [...prev[slot], item.id] };
    });
  }

  /** v4 `:499-512` — picking from a slot row wears the item (fills every slot
   *  it covers) rather than dropping it into the one row. Pure state. */
  protected handleSlotAdd(slot: WardrobeSlotType, itemId: string): void {
    if (!this.isInChat) return;
    const item = this.itemsById().get(itemId);
    this.updateLiveStaged((prev) =>
      item
        ? wearItemIntoSlots(prev, item)
        : prev[slot].includes(itemId)
          ? prev
          : { ...prev, [slot]: [...prev[slot], itemId] },
    );
  }

  /** v4 `:514-520` — pure state; fires no route. */
  protected handleSlotRemove(slot: WardrobeSlotType, itemId: string): void {
    if (!this.isInChat) return;
    this.updateLiveStaged((prev) => ({ ...prev, [slot]: prev[slot].filter((id) => id !== itemId) }));
  }

  /** v4 `:522-528`. */
  protected handleSlotClear(slot: WardrobeSlotType): void {
    if (!this.isInChat) return;
    this.updateLiveStaged((prev) => ({ ...prev, [slot]: [] }));
  }

  // ---------------------------------------------------------------------------
  // Fitting-room mutations — transient; never hit the equip API (v4 :531-587).
  // ---------------------------------------------------------------------------
  protected fittingAdd(slot: WardrobeSlotType, itemId: string): void {
    const item = this.itemsById().get(itemId);
    if (!item) return;
    this.fittingSlots.update((prev) => wearItemIntoSlots(prev, item));
  }

  protected fittingRemove(slot: WardrobeSlotType, itemId: string): void {
    this.fittingSlots.update((prev) => ({
      ...prev,
      [slot]: prev[slot].filter((id) => id !== itemId),
    }));
  }

  protected fittingClear(slot: WardrobeSlotType): void {
    this.fittingSlots.update((prev) => ({ ...prev, [slot]: [] }));
  }

  /** v4 `:551-564`. */
  protected fittingResetToWorn(): void {
    const characterId = this.selectedCharacterId();
    if (!characterId) return;
    const wornSlots = this.outfit.outfitState()[characterId]?.slots;
    const target = wornSlots ? cloneSlots(wornSlots) : cloneSlots(EMPTY_EQUIPPED_SLOTS);
    if (
      !equippedSlotsEqual(this.fittingSlots(), target) &&
      !window.confirm('Discard your composition and start from what’s currently worn?')
    ) {
      return;
    }
    this.fittingSlots.set(target);
  }

  /** v4 `:566-577`. */
  protected fittingResetToDefaults(): void {
    const target = buildDefaultOutfit(this.items());
    if (
      !equippedSlotsEqual(this.fittingSlots(), target) &&
      !window.confirm('Discard your composition and start from this character’s default outfit?')
    ) {
      return;
    }
    this.fittingSlots.set(target);
  }

  /** v4 `:579-587`. */
  protected fittingClearAll(): void {
    if (
      !equippedSlotsEqual(this.fittingSlots(), EMPTY_EQUIPPED_SLOTS) &&
      !window.confirm('Empty every slot in the Outfit Builder?')
    ) {
      return;
    }
    this.fittingSlots.set(freshSlots());
  }

  /**
   * v4 `:589-609` — open the editor in bundle mode with `componentItemIds`
   * pre-populated from the staged slots (composite ids preserved by
   * reference; the server's cycle detection + union-of-types handles it).
   */
  protected handleSaveAsOutfit(): void {
    if (!this.selectedCharacterId()) return;
    const seen = new Set<string>();
    const components: string[] = [];
    const slots = this.fittingSlots();
    for (const slot of ['top', 'bottom', 'footwear', 'accessories'] as const) {
      for (const id of slots[slot]) {
        if (!seen.has(id)) {
          seen.add(id);
          components.push(id);
        }
      }
    }
    this.createBundleComponents.set(components);
    this.creatingNew.set('create-bundle');
  }

  protected handleLiveTakeOffBundle(bundle: EquippedBundle): void {
    this.updateLiveStaged((prev) => takeOffBundleFromSlots(prev, bundle));
  }

  protected handleLiveBreakApartBundle(bundle: EquippedBundle): void {
    this.updateLiveStaged((prev) => breakApartBundleInSlots(prev, bundle, this.itemsById()));
  }

  protected handleFittingTakeOffBundle(bundle: EquippedBundle): void {
    this.fittingSlots.update((prev) => takeOffBundleFromSlots(prev, bundle));
  }

  protected handleFittingBreakApartBundle(bundle: EquippedBundle): void {
    this.fittingSlots.update((prev) => breakApartBundleInSlots(prev, bundle, this.itemsById()));
  }

  /**
   * Flush staged Live-tab edits on close (v4 `:648-685`): for each character
   * whose staged slots differ from their baseline, fire ONE `set_all` (which
   * triggers a single avatar regen + Aurora announcement server-side). If
   * nothing is dirty, no requests go out. Returns true when every commit
   * succeeded (or there was nothing to commit).
   */
  async flushStagedLiveOutfits(): Promise<boolean> {
    const chatId = this.chatId();
    if (!chatId) return true;
    const dirty: Array<{ characterId: string; slots: EquippedSlots }> = [];
    for (const [characterId, slots] of Object.entries(this.liveStagedByChar())) {
      const baseline = this.liveBaselineByChar[characterId];
      if (!baseline) continue;
      if (!equippedSlotsEqual(slots, baseline)) {
        dirty.push({ characterId, slots });
      }
    }
    if (dirty.length === 0) return true;

    let allOk = true;
    for (const { characterId, slots } of dirty) {
      try {
        await dispatchWardrobe(this.core, {
          type: 'chatEquip',
          chatId,
          characterId,
          mode: 'set_all',
          slots,
        });
        this.outfit.invalidateWardrobe(characterId);
        this.liveBaselineByChar[characterId] = cloneSlots(slots);
      } catch (err) {
        this.toasts.showError(err instanceof Error ? err.message : 'Failed to update outfit');
        allOk = false;
      }
    }
    await this.outfit.refreshOutfit();
    return allOk;
  }

  /** v4 `:687-692` — Done: flush, then close only when every commit stuck. */
  protected requestClose(): void {
    void (async () => {
      const ok = await this.flushStagedLiveOutfits();
      if (ok) this.closed.emit();
    })();
  }

  /**
   * v4 `:700-722` — wear the fitting-room composition: atomic replace via
   * `mode: 'set_all'`. On success the dialog closes — Aurora + avatar gen
   * fire immediately server-side.
   */
  protected async wearFitting(): Promise<void> {
    const characterId = this.selectedCharacterId();
    const chatId = this.chatId();
    if (!characterId || !chatId) return;
    try {
      await dispatchWardrobe(this.core, {
        type: 'chatEquip',
        chatId,
        characterId,
        mode: 'set_all',
        slots: this.fittingSlots(),
      });
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to wear this outfit');
      return;
    }
    this.toasts.showSuccess('Worn!');
    this.outfit.invalidateWardrobe(characterId);
    await this.outfit.refreshOutfit();
    this.closed.emit();
  }

  /**
   * v4 `:724-730` — row buttons route to the Builder's transient state when
   * the builder tab is active (or always out of chat), else to the Live-tab
   * staging map. Both paths defer the server commit until Done / Try on.
   */
  protected readonly useFittingActions = computed(
    () => !this.isInChat || this.rightTab() === 'builder',
  );

  /** v4 `:732-743`. */
  protected rowEquip(item: WardrobeItemDto): void {
    if (this.useFittingActions()) {
      this.fittingSlots.update((prev) => wearItemIntoSlots(prev, item));
      return;
    }
    this.handleEquipItem(item);
  }

  /** v4 `:745-754`. */
  protected rowAddToSlot(item: WardrobeItemDto, slot: WardrobeSlotType): void {
    if (this.useFittingActions()) {
      this.fittingAdd(slot, item.id);
      return;
    }
    this.handleAddToSlot(item, slot);
  }

  // ---------------------------------------------------------------------------
  // Avatar generation (v4 :759-817)
  // ---------------------------------------------------------------------------
  protected async handleGenerateAvatar(): Promise<void> {
    const characterId = this.selectedCharacterId();
    if (!characterId) return;
    this.previewUrl.set(null);
    this.previewFilename.set(null);
    this.generating.set(true);

    try {
      const chatId = this.chatId();
      if (this.isInChat && chatId) {
        // The fitting-room slots flow through as a one-shot override; the
        // chat's stored `equippedOutfit` is unaffected (v4 :766-768).
        try {
          await dispatchWardrobe(this.core, {
            type: 'chatRegenerateAvatar',
            chatId,
            characterId,
            equippedSlots: this.fittingSlots(),
            ...(this.selectedImageProfileId()
              ? { imageProfileId: this.selectedImageProfileId()! }
              : {}),
          });
          this.toasts.showSuccess(
            'Avatar generation queued — the new portrait will appear shortly.',
          );
        } catch (err) {
          this.toasts.showError(err instanceof Error ? err.message : 'Failed to queue avatar generation');
        }
      } else {
        try {
          const data = await dispatchWardrobe(this.core, {
            type: 'wardrobePreviewAvatar',
            characterId,
            equippedSlots: this.fittingSlots(),
            ...(this.selectedImageProfileId()
              ? { imageProfileId: this.selectedImageProfileId()! }
              : {}),
          });
          const url = data['url'] as string | undefined;
          if (!url) throw new Error('Failed to generate preview');
          // The server URL is relative — resolve through apiUrl (finding #12).
          this.previewUrl.set(apiUrl(url));
          const character = this.characters().find((c) => c.id === characterId);
          this.previewFilename.set(
            `${character?.name.replace(/[^a-zA-Z0-9]/g, '_') ?? 'avatar'}_preview.webp`,
          );
        } catch (err) {
          this.toasts.showError(err instanceof Error ? err.message : 'Failed to generate preview');
        }
      }
    } finally {
      this.generating.set(false);
    }
  }

  // ---------------------------------------------------------------------------
  // Render helpers
  // ---------------------------------------------------------------------------

  /** What the Live tab actually paints (v4 `:822-831`): staged slots once
   *  seeding captured a baseline, else the worn snapshot, else empty. */
  protected readonly liveDisplaySlots = computed<EquippedSlots>(() => {
    const characterId = this.selectedCharacterId();
    if (!characterId) return EMPTY_EQUIPPED_SLOTS as EquippedSlots;
    return (
      this.liveStagedByChar()[characterId] ??
      (this.booted ? this.outfit.outfitState()[characterId]?.slots : undefined) ??
      (EMPTY_EQUIPPED_SLOTS as EquippedSlots)
    );
  });

  protected readonly editorInitialMode = computed<'single' | 'bundle' | undefined>(() => {
    switch (this.creatingNew()) {
      case 'create-bundle':
        return 'bundle';
      case 'create-single':
        return 'single';
      default:
        return undefined;
    }
  });

  // ---------------------------------------------------------------------------
  // Stacked-dialog wiring (v4 :1213-1269)
  // ---------------------------------------------------------------------------
  protected closeEditor(): void {
    this.editingItem.set(null);
    this.creatingNew.set(null);
    this.createBundleComponents.set([]);
  }

  protected async onEditorSaved(): Promise<void> {
    this.closeEditor();
    await this.reloadCurrentItems();
    const characterId = this.selectedCharacterId();
    if (this.isInChat && characterId) {
      this.outfit.invalidateWardrobe(characterId);
      await this.outfit.refreshOutfit();
    }
  }

  protected openTransfer(item: WardrobeItemDto, intent: TransferMode): void {
    this.transferringItem.set(item);
    this.transferIntent.set(intent);
  }

  protected closeTransfer(): void {
    this.transferringItem.set(null);
    this.transferIntent.set(null);
  }

  protected async onTransferred(): Promise<void> {
    this.closeTransfer();
    await this.reloadCurrentItems();
    const characterId = this.selectedCharacterId();
    if (this.isInChat && characterId) {
      this.outfit.invalidateWardrobe(characterId);
      await this.outfit.refreshOutfit();
    }
  }

  // ---------------------------------------------------------------------------
  // Close plumbing
  // ---------------------------------------------------------------------------
  protected onBackdrop(): void {
    if (this.editorOpen()) return;
    this.requestClose();
  }

  protected onEscape(event: Event): void {
    if (this.resetMenuOpen()) {
      event.stopPropagation();
      event.preventDefault();
      this.resetMenuOpen.set(false);
      return;
    }
    // In tab mode v4 renders no BaseModal, so Escape never closes the surface
    // (only the Reset menu above). The reset-menu arm still fires either way.
    if (this.asTab()) return;
    if (this.editorOpen()) return;
    this.requestClose();
  }

  /** Close the Reset menu on outside click (v4 :846-866). */
  protected onDocumentMouseDown(): void {
    if (this.resetMenuOpen()) this.resetMenuOpen.set(false);
  }
}

/**
 * Wrapper component mounted once in the shell (v4 `WardrobeControlDialog`,
 * `wardrobe-control-dialog.tsx:87-98`; mounted at `app-layout.tsx:126-138`).
 * Renders nothing while closed. The keyed `@for` reproduces v4's inner `key`
 * (`:92`): when `${characterId}|${chatId}` changes, the tracked item changes,
 * so Angular destroys and recreates the inner — discarding all staged state.
 */
@Component({
  selector: 'qt-wardrobe-control-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [WardrobeControlDialogInner],
  template: `
    @if (dialog.isOpen()) {
      @for (key of [dialog.remountKey()]; track key) {
        <qt-wardrobe-dialog-inner
          [initialCharacterId]="dialog.context()?.characterId ?? null"
          [chatId]="dialog.context()?.chatId ?? null"
          (closed)="dialog.close()"
        />
      }
    }
  `,
})
export class WardrobeControlDialog {
  protected readonly dialog = inject(WardrobeDialogService);
}

/**
 * The Wardrobe as a left-rail workspace tab (v4 `WardrobeView`,
 * `wardrobe-control-dialog.tsx:105-114`). A thin re-skin of the inner dialog in
 * bare tab chrome: browse/edit only — `chatId` is null, so there is no
 * "wearing now" column and no equip route ever fires (the chat-scoped path
 * keeps the floating dialog, which can change what a character is actively
 * wearing). Singleton. The rail opens it with NO payload (the inner then
 * auto-selects the first character); a `characterId` payload preselects one.
 */
@Component({
  selector: 'qt-wardrobe-tab-view',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [WardrobeControlDialogInner],
  template: `
    <qt-wardrobe-dialog-inner
      [asTab]="true"
      [initialCharacterId]="characterId() ?? null"
      [chatId]="null"
      (closed)="noop()"
    />
  `,
})
export class WardrobeTabView {
  /** From `WardrobeTabPayload.characterId` (v4 `WardrobeView` prop). */
  readonly characterId = input<string | undefined>(undefined);

  protected noop(): void {
    /* v4 passes `onClose={() => {}}` — a tab has no self-close. */
  }
}
