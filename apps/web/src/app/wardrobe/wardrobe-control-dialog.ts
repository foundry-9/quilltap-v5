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
import { RealtimeService } from '../core/realtime.service';
import type { CharacterListItem, WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { characterAvatarSrc } from '../screens/characters/characters.api';
import { AvatarGenerationPane, type ImageProfileSummary } from './avatar-generation-pane';
import {
  buildDefaultOutfitWithCredit,
  cloneSlots,
  EMPTY_EQUIPPED_SLOTS,
  freshSlots,
  type EquippedSlots,
} from './equipped-slots';
import { WARDROBE_SLOT_TYPES } from './slot-meta';
import { createOutfitStore, type OutfitStore } from './outfit-store';
import {
  addToSlotGesture,
  appendWornBundleIds,
  buildSetAllEquipBody,
  classifyStagedOutfits,
  createComposerHandlers,
  equippedSlotsEqual,
  rebaseStagedGestures,
  wearGesture,
  wearRefusal,
  type StagedGesture,
} from './staged-live-outfits';
import { OutfitComposer } from './outfit-composer';
import { OutfitHistoryTab } from './outfit-history-tab';
import { WardrobeDialogService } from './wardrobe-dialog.service';
import { WardrobeBrowser, type WardrobeRowWearControls } from './wardrobe-browser';
import { WardrobeItemOverlays } from './wardrobe-item-overlays';
import { createWardrobeItemActions } from './wardrobe-item-actions';
import { createWardrobeListData } from './wardrobe-list-data';
import {
  GENERAL_CONTAINER,
  GENERAL_WARDROBE_NAME,
  decodeWardrobeContainer,
  encodeWardrobeContainer,
  type WardrobeContainer,
} from './wardrobe-container';
import { dispatchWardrobe } from './wardrobe.api';
import { WardrobeInstructionsSection } from './wardrobe-instructions-section';
import { ToastService } from '../ui/toast.service';
import { onTabActivated } from '../workspace/workspace-contract';

interface CharacterSummary {
  id: string;
  name: string;
  avatarUrl?: string | null;
}

/** A project or group offered as a browsable shared wardrobe container
 *  (v4 `wardrobe-control-dialog.tsx:81-85`). */
interface ContainerSummary {
  id: string;
  name: string;
}

type RightTab = 'live' | 'builder' | 'history';

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
 * THE PHASE-F SPLIT (v4 `9ecebf986` + `4a38222e6`, P4.D285): the left column
 * is the shared `qt-wardrobe-browser` (folders, tier rows, select mode), the
 * editor / transfer / picture overlays are `qt-wardrobe-item-overlays`, the
 * row actions and every overlay's open state are the actions store
 * (`wardrobe-item-actions.ts` — its `overlayOpen` counts the Move-to-folder
 * picker, bug 229), and the lists are the list-data store
 * (`wardrobe-list-data.ts`). The dialog lays them out and keeps the staging.
 * Every wear gesture is a composer gesture (`staged-live-outfits.ts`) and is
 * refused for an archived garment with v4's sentence (bug 191, `wearRefusal`).
 *
 * BUG 198 (v4 `e23fb095b`): the worn snapshot is read per mount (the outfit
 * store is built at boot and the inner dialog is destroyed on close, so no
 * earlier visit's snapshot can seed the staging — measured, not assumed), and
 * re-read on the `chats` realtime topic, which a tool-driven outfit change
 * fires (v4's `chats` → `wardrobe.outfit(id)` topic row; v5 has no outfit
 * query key, so the dialog subscribes itself — `realtime-topic-map.ts`
 * unchanged).
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
    OutfitHistoryTab,
    WardrobeBrowser,
    WardrobeInstructionsSection,
    WardrobeItemOverlays,
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
          <!-- Container selector — every place a wardrobe item can live gets an
               entry: characters, Quilltap General, projects, and groups
               (v4 :1061-1135). -->
          <div class="flex flex-col gap-3 mb-3">
            <div class="flex items-center gap-2">
              <label for="wardrobe-container-select" class="text-sm qt-text-secondary">
                Wardrobe:
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
                id="wardrobe-container-select"
                class="qt-select flex-1 max-w-md"
                (change)="pickContainer($any($event.target).value)"
              >
                @if (!selectedContainer()) {
                  <option value="" disabled selected>Select a wardrobe</option>
                }
                @if (characters().length > 0) {
                  <optgroup label="Characters">
                    @for (c of characters(); track c.id) {
                      <option
                        [value]="encodeContainer({ scope: 'character', id: c.id })"
                        [selected]="isSelectedContainer('character', c.id)"
                      >
                        {{ c.name }}
                      </option>
                    }
                  </optgroup>
                }
                <optgroup label="General">
                  <option
                    [value]="generalContainerValue"
                    [selected]="isSelectedContainer('general', null)"
                  >
                    Quilltap General
                  </option>
                </optgroup>
                @if (projects().length > 0) {
                  <optgroup label="Projects">
                    @for (p of projects(); track p.id) {
                      <option
                        [value]="encodeContainer({ scope: 'project', id: p.id })"
                        [selected]="isSelectedContainer('project', p.id)"
                      >
                        {{ p.name }}
                      </option>
                    }
                  </optgroup>
                }
                @if (groups().length > 0) {
                  <optgroup label="Groups">
                    @for (g of groups(); track g.id) {
                      <option
                        [value]="encodeContainer({ scope: 'group', id: g.id })"
                        [selected]="isSelectedContainer('group', g.id)"
                      >
                        {{ g.name }}
                      </option>
                    }
                  </optgroup>
                }
              </select>
            </div>
            @if (!isCharacterScope() && selectedContainer(); as container) {
              <p class="qt-text-xs qt-text-secondary px-1">
                Browsing a shared wardrobe — items here can be worn by every character who can
                reach this {{ container.scope === 'general' ? 'library' : container.scope }}.
                Edit, duplicate, or delete them freely; pick a character above to dress someone.
              </p>
            }
          </div>

          <!-- Optional dressing instructions for this container — consulted when
               a character chooses their own opening outfit; nearest tier wins
               (v4 :1139-1142). -->
          <qt-wardrobe-instructions-section [container]="selectedContainer()" />

          <div class="grid md:grid-cols-2 gap-4">
            <!-- LEFT: the shared browser (v4 4a38222e6 :478-497) -->
            <qt-wardrobe-browser
              [container]="selectedContainer()"
              [data]="data"
              [actions]="actions"
              [showArchived]="showArchived()"
              (showArchivedChange)="showArchived.set($event)"
              [wear]="wearControls()"
            />

            <!-- RIGHT: Live outfit / Outfit Builder / History — Builder always
                 present; Live outfit and History only when there is a chat. -->
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
                  @if (isInChat) {
                    <button
                      type="button"
                      (click)="rightTab.set('history')"
                      class="qt-tab"
                      [class.qt-tab-active]="rightTab() === 'history'"
                    >History</button>
                  }
                </div>

                <!-- One chain, v4's ternary (8d8618819 :537-546): History FIRST,
                     then Live, else the Builder body. -->
                @if (rightTab() === 'history' && isInChat && chatId(); as cid) {
                  <qt-outfit-history-tab
                    [chatId]="cid"
                    [characterId]="selectedCharacterId()!"
                    [characterName]="selectedCharacter()?.name ?? 'this character'"
                    [hasStagedEdits]="isLiveDirty(selectedCharacterId()!)"
                    [discardStagedEdits]="discardFor(selectedCharacterId()!)"
                    [requestConfirmation]="historyConfirm"
                  />
                } @else if (rightTab() === 'live' && isInChat) {
                  <div class="space-y-2 mb-3">
                    <p class="qt-text-xs qt-text-secondary px-1">
                      Edits stage here and apply when you click Done. Nothing happens until then.
                    </p>
                    <qt-outfit-composer
                      [items]="items()"
                      [slots]="liveDisplaySlots()"
                      (addToSlot)="liveHandlers.onAddToSlot($event.slot, $event.itemId)"
                      (removeFromSlot)="liveHandlers.onRemoveFromSlot($event.slot, $event.itemId)"
                      (clearSlot)="liveHandlers.onClearSlot($event)"
                      (takeOffBundle)="liveHandlers.onTakeOffBundle($event)"
                      (breakApartBundle)="liveHandlers.onBreakApartBundle($event)"
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
                      (addToSlot)="fittingHandlers.onAddToSlot($event.slot, $event.itemId)"
                      (removeFromSlot)="fittingHandlers.onRemoveFromSlot($event.slot, $event.itemId)"
                      (clearSlot)="fittingHandlers.onClearSlot($event)"
                      (takeOffBundle)="fittingHandlers.onTakeOffBundle($event)"
                      (breakApartBundle)="fittingHandlers.onBreakApartBundle($event)"
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

    <!-- The overlays the browser's row actions open (v4 4a38222e6 :650-655). -->
    <qt-wardrobe-item-overlays
      [container]="selectedContainer()"
      [containerLabel]="selectedContainerLabel()"
      [data]="data"
      [actions]="actions"
    />
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

  protected readonly characters = signal<CharacterSummary[]>([]);
  protected readonly projects = signal<ContainerSummary[]>([]);
  protected readonly groups = signal<ContainerSummary[]>([]);
  /**
   * Which wardrobe is being browsed: a character's merged view, or one shared
   * container (Quilltap General, a project, a group) edited in place
   * (v4 `:209-227`). `selectedCharacterId` is DERIVED from it — a shared
   * container has no character, which is what hides the right-hand outfit
   * column and the character-only actions.
   */
  protected readonly selectedContainer = signal<WardrobeContainer | null>(null);
  protected readonly isCharacterScope = computed(
    () => this.selectedContainer()?.scope === 'character',
  );
  protected readonly selectedCharacterId = computed(() =>
    this.isCharacterScope() ? (this.selectedContainer()?.id ?? null) : null,
  );

  /**
   * "Show archived" (v4 `:229-236`). Flipping it re-reads every tier with
   * `includeArchived` rather than filtering what is already loaded — the server
   * owns the hiding, so this list can never disagree with the API.
   */
  protected readonly showArchived = signal(false);

  /** v4 `useWardrobeListData(selectedContainer, { chatId, includeArchived })` (`:237`). */
  protected readonly data = createWardrobeListData(this.core, {
    container: () => this.selectedContainer(),
    chatId: () => this.chatId(),
    includeArchived: () => this.showArchived(),
  });
  /** The character's wearable pool (empty in a shared view) — v4 `items` (`:238`). */
  protected readonly items = this.data.characterItems;

  /**
   * v4 `useWardrobeItemActions(…)` (`:254-259`). v4's imperative
   * `showConfirmation` is `window.confirm` here (the recorded divergence at
   * the top of this file).
   */
  protected readonly actions = createWardrobeItemActions({
    core: this.core,
    toasts: this.toasts,
    container: () => this.selectedContainer(),
    listItems: () => this.data.listItems(),
    confirm: (message) => window.confirm(message),
    reloadLists: () => this.data.reload(),
    reloadOutfit: () => this.reloadOutfit(),
  });

  protected readonly imageProfiles = signal<ImageProfileSummary[]>([]);
  protected readonly selectedImageProfileId = signal<string | null>(null);
  protected readonly generating = signal(false);
  protected readonly previewUrl = signal<string | null>(null);
  protected readonly previewFilename = signal<string | null>(null);

  /** Fitting room — transient; never hits the equip API (v4 `useFittingRoom`). */
  protected readonly fittingSlots = signal<EquippedSlots>(freshSlots());
  /**
   * The outfits (bundles) the Builder's composition put on — the claim its Try
   * on sends as `wornBundleIds` (v4 `3ee3b1342` `:294`). Never rendered, so a
   * plain field: appended by every wear gesture, reset by Reset to worn /
   * Clear all, set from the defaults' credit by Reset to defaults.
   */
  private fittingWornBundleIds: string[] = [];
  /** v4 `:203` — the load-bearing chat-awareness flag. Fixed per mount (the
   *  remount key includes chatId). */
  protected isInChat = false;
  protected readonly rightTab = signal<RightTab>('builder');
  protected readonly resetMenuOpen = signal(false);
  private fittingSeedKey: string | null = null;

  // -------- Staged Live-outfit edits (v4 `useStagedLiveOutfits`) --------
  //
  // Per-slot tweaks on the Live tab used to call the equip API one at a time,
  // and the server fired a fresh avatar regen + Aurora announcement on every
  // call. Now: every Live-tab gesture stages here, keyed by character. On
  // Done, each character whose staged slots differ from their baseline gets
  // one `set_all` — so at most one announcement and one regen per character
  // per dialog session.
  protected readonly liveStagedByChar = signal<Record<string, EquippedSlots>>({});
  private readonly liveBaselineByChar: Record<string, EquippedSlots> = {};
  private readonly liveSeededByChar = new Set<string>();
  /**
   * Gestures made before the worn snapshot arrived, keyed by character (v4
   * `pendingRef`). In that window there is nothing to stage onto, so the
   * gesture paints against an empty fallback and is recorded here; the seeding
   * effect replays it onto the real slots. Without this the fast click is
   * either overwritten by the first seed (Bug 61's silent loss) or committed
   * against an empty base, undressing everything the user never touched.
   */
  private readonly pendingLiveMutators: Record<string, StagedGesture[]> = {};
  /**
   * The bundles each character's staged Live gestures put on (v4 `3ee3b1342`
   * `:332`), sent beside the slots on Done. Rebuilt by the seed's rebase,
   * deleted for a character whose commit succeeded.
   */
  private readonly liveWornBundlesByChar: Record<string, string[]> = {};
  /**
   * Bumped by `discardLive` so the seeding effect runs again for a character
   * whose staging was thrown away, even if the snapshot has not changed yet
   * (v4 `useStagedLiveOutfits` `reseedTick`, `8d8618819` `:94-96`).
   */
  private readonly liveReseedTick = signal(0);

  protected outfit!: OutfitStore;
  private booted = false;

  /** The worn slots of the character being dressed, in chat (v4 `wornSlots`, `:268`). */
  private wornSlotsFor(characterId: string | null): EquippedSlots | undefined {
    return this.isInChat && this.booted ? this.outfit.slotsFor(characterId) : undefined;
  }

  constructor() {
    // One-time boot once the inputs resolve (the component remounts per
    // context key, so this runs exactly once per context).
    effect(() => {
      const initialCharacterId = this.initialCharacterId();
      const chatId = this.chatId();
      if (this.booted) return;
      untracked(() => {
        this.isInChat = chatId !== null;
        this.rightTab.set(this.isInChat ? 'live' : 'builder');
        this.selectedContainer.set(
          initialCharacterId ? { scope: 'character', id: initialCharacterId } : null,
        );
        // Bug 198: a fresh store per mount — no earlier visit's snapshot.
        this.outfit = createOutfitStore(this.core, chatId);
        this.booted = true;
        void this.loadCharacters();
        void this.loadSharedContainers();
        void this.loadImageProfiles();
        void this.outfit.refreshOutfit();
      });
    });

    // Fitting-room seeding (v4 `useFittingRoom`): once per character. In chat
    // wait for the worn snapshot; out of chat wait for items so the defaults
    // can be picked out.
    effect(() => {
      const characterId = this.selectedCharacterId();
      const items = this.items();
      if (this.booted) this.outfit.equippedOutfit();
      if (!characterId) return;
      const wornSlots = this.wornSlotsFor(characterId);
      if (this.isInChat && !wornSlots) return;
      if (items.length === 0 && !wornSlots) return;

      const seedKey = `${characterId}|${this.chatId() ?? 'no-chat'}`;
      if (this.fittingSeedKey === seedKey) return;
      this.fittingSeedKey = seedKey;

      // v4 `:463-468`: worn slots carry no claim; the defaults credit the
      // default bundles they dissolved.
      const seed =
        this.isInChat && wornSlots
          ? { slots: cloneSlots(wornSlots), wornBundles: [] }
          : buildDefaultOutfitWithCredit(items);
      this.fittingWornBundleIds = seed.wornBundles.map((b) => b.id);
      untracked(() => this.fittingSlots.set(seed.slots));
    });

    // Seed staged Live slots once a worn snapshot exists for this character.
    // The baseline is captured separately so the Done flush can skip no-op
    // commits; the seeded-set gates re-seeding so a refetch doesn't blow away
    // in-progress edits — and anything staged *before* this first seed is
    // replayed onto the snapshot rather than discarded by it, since the item
    // list is clickable well before the snapshot arrives (Bug 61).
    effect(() => {
      const characterId = this.selectedCharacterId();
      // v4 `reseedTick` dep (`useStagedLiveOutfits.ts:118`) — `discardLive`
      // deletes from a plain Set, which re-runs nothing.
      this.liveReseedTick();
      if (this.booted) this.outfit.equippedOutfit();
      if (!this.isInChat || !characterId) return;
      const wornSlots = this.wornSlotsFor(characterId);
      if (!wornSlots) return;
      const seedKey = `${characterId}|${this.chatId() ?? 'no-chat'}`;
      if (this.liveSeededByChar.has(seedKey)) return;
      this.liveSeededByChar.add(seedKey);
      this.liveBaselineByChar[characterId] = cloneSlots(wornSlots);
      const pending = this.pendingLiveMutators[characterId] ?? [];
      delete this.pendingLiveMutators[characterId];
      // v4 `:492-494` — the claims are rebuilt from exactly the replayed
      // gestures, so none outlives its staged state.
      const seed = rebaseStagedGestures(wornSlots, pending);
      this.liveWornBundlesByChar[characterId] = seed.wornBundleIds;
      untracked(() =>
        this.liveStagedByChar.update((prev) => ({
          ...prev,
          [characterId]: seed.slots,
        })),
      );
    });

    // As a rail-opened workspace tab, navigating back refreshes the lists.
    // Safe mid-edit: the fitting-room/live seeding is gated, so a reload never
    // blows away staged slots. v4 `:263-265` (`actions.refresh()`).
    onTabActivated(() => {
      void this.actions.refresh();
    });

    const realtime = inject(RealtimeService);
    // P4.D261 (R-H, v4 `b3f937076` `job-topics.ts`): a finished wardrobe-picture
    // job publishes `characters` (id = the item's character) + `mountPoints`,
    // and v4's wardrobe views watch both. v5's list is signal-held, so no
    // TanStack key the topic map resolves can reach it — the dialog subscribes
    // itself, scoped to the selected character (an id-less, collection-wide
    // hint fires too; it says nothing about which rows). `mountPoints` resolves
    // to nothing in the SPA (`realtime-topic-map.ts`) and stays so.
    realtime.onTopic(
      'characters',
      () => void this.data.reload(),
      () => this.selectedCharacterId(),
    );
    // Bug 198's realtime half (v4 `e23fb095b` `topic-map.ts:72-76`): a
    // tool-driven outfit change lands as messages (the tool rows + Aurora's
    // announcement), which publish `chats`; the worn snapshot re-reads so the
    // un-staged display and "Reset to worn" point at what the chat really
    // wears. The staging itself never re-seeds (Bug 61's contract).
    realtime.onTopic(
      'chats',
      () => void this.reloadOutfit(),
      () => this.chatId(),
    );
  }

  /** v4's `wardrobe.outfit(chatId)` invalidation — chat-only. */
  private async reloadOutfit(): Promise<void> {
    if (!this.isInChat || !this.booted) return;
    await this.outfit.refreshOutfit();
  }

  // ---------------------------------------------------------------------------
  // Loads (v4 `useWardrobeCatalogue`, `:79-116`)
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
      // Auto-select if nothing was specified: the first character, or the
      // General library on a characterless instance (v4 `:313-317`).
      if (!this.selectedContainer()) {
        this.selectedContainer.set(
          sorted[0] ? { scope: 'character', id: sorted[0].id } : GENERAL_CONTAINER,
        );
      }
    } catch {
      this.characters.set([]);
    }
  }

  /**
   * v4 `:319-347` — the shared half of the container catalogue: every project
   * and group gets a menu entry. Each read fails soft and independently; a
   * missing tier simply contributes no options.
   */
  private async loadSharedContainers(): Promise<void> {
    const [projects, groups] = await Promise.all([
      this.core.dispatchData({ type: 'projectList' }).catch(() => null),
      this.core.dispatchData({ type: 'groupList' }).catch(() => null),
    ]);
    if (projects) {
      this.projects.set(
        ((projects['projects'] as ContainerSummary[] | undefined) ?? [])
          .map((p) => ({ id: p.id, name: p.name || 'Untitled project' }))
          .sort((a, b) => a.name.localeCompare(b.name)),
      );
    }
    if (groups) {
      this.groups.set(
        ((groups['groups'] as ContainerSummary[] | undefined) ?? [])
          .map((g) => ({ id: g.id, name: g.name || 'Untitled group' }))
          .sort((a, b) => a.name.localeCompare(b.name)),
      );
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

  protected readonly selectedCharacter = computed(
    () => this.characters().find((c) => c.id === this.selectedCharacterId()) ?? null,
  );

  /** Display name of the browsed container, for the editor's pinned note
   *  (v4 `:374-386`). */
  protected readonly selectedContainerLabel = computed(() => {
    const container = this.selectedContainer();
    if (!container) return '';
    switch (container.scope) {
      case 'character':
        return this.selectedCharacter()?.name ?? '';
      case 'general':
        return GENERAL_WARDROBE_NAME;
      case 'project':
        return this.projects().find((p) => p.id === container.id)?.name ?? 'This project';
      case 'group':
        return this.groups().find((g) => g.id === container.id)?.name ?? 'This group';
    }
  });

  private readonly itemsById = computed(() => new Map(this.items().map((i) => [i.id, i])));

  /**
   * While an overlay is up (the editor, the move/copy dialog, the picture, the
   * Move-to-folder picker), backdrop / Escape must not close the outer dialog
   * (v4 `overlayOpen`, `:390`; bug 229 — the picker counts from day one, R-O).
   */
  protected readonly overlayOpen = computed(() => this.actions.overlayOpen());

  // ---------------------------------------------------------------------------
  // The wear gestures (v4 `:290-350`)
  // ---------------------------------------------------------------------------

  /**
   * An archived garment is retired: every wear gesture refuses it with the
   * same words the equip route and `wardrobe_wear` use (bug 191).
   */
  private readonly refuseIfUnwearable = (item: WardrobeItemDto): boolean => {
    const refusal = wearRefusal(item);
    if (refusal) this.toasts.showError(refusal);
    return refusal !== null;
  };

  /** Land a gesture on the Live tab's staging (v4 `useStagedLiveOutfits.apply`). */
  private applyLive(gesture: StagedGesture): void {
    const characterId = this.selectedCharacterId();
    if (!this.isInChat || !characterId) return;
    // Until the worn snapshot has seeded there is no honest base to stage
    // onto: the gesture paints against an empty fallback, and is recorded so
    // the seed can replay it onto the real slots (v4 `:111-116`).
    const seedKey = `${characterId}|${this.chatId() ?? 'no-chat'}`;
    if (!this.liveSeededByChar.has(seedKey)) {
      const queued = this.pendingLiveMutators[characterId] ?? [];
      this.pendingLiveMutators[characterId] = [...queued, gesture];
    }
    // v4 `3ee3b1342` — the outfits this gesture put on.
    if (gesture.wornBundleIds.length > 0) {
      this.liveWornBundlesByChar[characterId] = appendWornBundleIds(
        this.liveWornBundlesByChar[characterId],
        gesture.wornBundleIds,
      );
    }
    this.liveStagedByChar.update((prev) => {
      const wornFallback = this.wornSlotsFor(characterId);
      const current =
        prev[characterId] ??
        (wornFallback ? cloneSlots(wornFallback) : cloneSlots(EMPTY_EQUIPPED_SLOTS));
      return { ...prev, [characterId]: gesture.mutate(current) };
    });
  }

  /** Land a gesture on the Outfit Builder (v4 `useFittingRoom.apply`). */
  private applyFitting(gesture: StagedGesture): void {
    this.fittingSlots.update((prev) => gesture.mutate(prev));
    if (gesture.wornBundleIds.length > 0) {
      this.fittingWornBundleIds = appendWornBundleIds(
        this.fittingWornBundleIds,
        gesture.wornBundleIds,
      );
    }
  }

  /** v4 `:299-308` — the two composers' handlers, one shared implementation. */
  protected readonly liveHandlers = createComposerHandlers({
    itemsById: () => this.itemsById(),
    apply: (g) => this.applyLive(g),
    refuse: this.refuseIfUnwearable,
  });
  protected readonly fittingHandlers = createComposerHandlers({
    itemsById: () => this.itemsById(),
    apply: (g) => this.applyFitting(g),
    refuse: this.refuseIfUnwearable,
  });

  /**
   * v4 `:326` — row buttons route to the Builder's transient state when the
   * builder tab is active (or always out of chat), else to the Live-tab
   * staging map. Both paths defer the server commit until Done / Try on.
   */
  protected readonly useFittingActions = computed(
    () => !this.isInChat || this.rightTab() === 'builder',
  );

  /** v4 `:328-337` — honours the item's `replace` flag across every slot it covers. */
  protected rowEquip(item: WardrobeItemDto): void {
    if (this.refuseIfUnwearable(item)) return;
    const gesture = wearGesture(item.types[0], item.id, this.itemsById());
    if (this.useFittingActions()) this.applyFitting(gesture);
    else this.applyLive(gesture);
  }

  /** v4 `:339-350`. */
  protected rowAddToSlot(item: WardrobeItemDto, slot: WardrobeSlotType): void {
    if (this.useFittingActions()) {
      this.fittingHandlers.onAddToSlot(slot, item.id);
      return;
    }
    if (!item.types.includes(slot)) return;
    if (this.refuseIfUnwearable(item)) return;
    this.applyLive(addToSlotGesture(slot, item, this.itemsById()));
  }

  /** v4 `:478-497` — the browser's row wear controls, character view only. */
  protected readonly wearControls = computed<WardrobeRowWearControls | undefined>(() =>
    this.isCharacterScope()
      ? {
          equipLabel: this.useFittingActions() ? 'Try on' : 'Wear',
          addAction: this.useFittingActions() ? 'add' : 'layer',
          onEquip: (item) => this.rowEquip(item),
          onAddToSlot: (item, slot) => this.rowAddToSlot(item, slot),
        }
      : undefined,
  );

  // ---------------------------------------------------------------------------
  // The Outfit Builder's resets (v4 `useFittingRoom`)
  // ---------------------------------------------------------------------------

  protected fittingResetToWorn(): void {
    const characterId = this.selectedCharacterId();
    if (!characterId) return;
    const wornSlots = this.wornSlotsFor(characterId);
    const target = wornSlots ? cloneSlots(wornSlots) : cloneSlots(EMPTY_EQUIPPED_SLOTS);
    if (
      !equippedSlotsEqual(this.fittingSlots(), target) &&
      !window.confirm('Discard your composition and start from what’s currently worn?')
    ) {
      return;
    }
    this.fittingSlots.set(target);
    this.fittingWornBundleIds = [];
  }

  /** v4 (+ `3ee3b1342` — the defaults' credit). */
  protected fittingResetToDefaults(): void {
    const { slots: target, wornBundles } = buildDefaultOutfitWithCredit(this.items());
    if (
      !equippedSlotsEqual(this.fittingSlots(), target) &&
      !window.confirm('Discard your composition and start from this character’s default outfit?')
    ) {
      return;
    }
    this.fittingSlots.set(target);
    this.fittingWornBundleIds = wornBundles.map((b) => b.id);
  }

  protected fittingClearAll(): void {
    if (
      !equippedSlotsEqual(this.fittingSlots(), EMPTY_EQUIPPED_SLOTS) &&
      !window.confirm('Empty every slot in the Outfit Builder?')
    ) {
      return;
    }
    this.fittingSlots.set(freshSlots());
    this.fittingWornBundleIds = [];
  }

  /**
   * v4 `:363-366` — open the editor in bundle mode with `componentItemIds`
   * pre-populated from the staged slots in slot order, distinct (v4
   * `useFittingRoom.componentIds`; composite ids preserved by reference — the
   * server's cycle detection + union-of-types handles it).
   */
  protected handleSaveAsOutfit(): void {
    if (!this.selectedCharacterId()) return;
    const seen = new Set<string>();
    const components: string[] = [];
    const slots = this.fittingSlots();
    for (const slot of WARDROBE_SLOT_TYPES) {
      for (const id of slots[slot]) {
        if (!seen.has(id)) {
          seen.add(id);
          components.push(id);
        }
      }
    }
    this.actions.openCreate('bundle', components);
  }

  /**
   * Flush staged Live-tab edits on close (v4 `useStagedLiveOutfits.flush`):
   * for each character whose staged slots differ from their baseline, fire ONE
   * `set_all` (which triggers a single avatar regen + Aurora announcement
   * server-side). If nothing is dirty, no requests go out.
   *
   * A character staged against no baseline at all is *not* clean — their worn
   * snapshot never arrived, so the staging was built on an empty fallback and
   * committing it would undress them. That case is put to the operator rather
   * than passed off as a successful save (Bug 61). Declining keeps the dialog
   * open, so a late snapshot can still seed and save; confirming falls through
   * to the normal dirty flush, having said plainly that the change is going.
   *
   * Returns true when every commit succeeded (or there was nothing to commit).
   */
  async flushStagedLiveOutfits(): Promise<boolean> {
    const chatId = this.chatId();
    if (!chatId) return true;
    const { dirty, unresolved } = classifyStagedOutfits(
      this.liveStagedByChar(),
      this.liveBaselineByChar,
      this.liveWornBundlesByChar,
    );

    if (unresolved.length > 0) {
      const names = unresolved
        .map((id) => this.characters().find((c) => c.id === id)?.name ?? 'this character')
        .join(', ');
      // v4 awaits its `showConfirmation`; v5's dialog uses the synchronous
      // native `window.confirm` throughout (the recorded divergence at the top
      // of this file). Copy is v4's verbatim.
      const discard = window.confirm(
        `Word of what ${names} is presently wearing never reached us, so your alterations cannot be saved. Close the wardrobe and let them go?`,
      );
      if (!discard) return false;
    }

    if (dirty.length === 0) return true;

    let allOk = true;
    for (const { characterId, slots, wornBundleIds } of dirty) {
      try {
        await dispatchWardrobe(this.core, {
          type: 'chatEquip',
          chatId,
          ...buildSetAllEquipBody(characterId, slots, wornBundleIds),
        });
        this.liveBaselineByChar[characterId] = cloneSlots(slots);
        // Committed: the claims went with the slots, so start the next round
        // clean (v4 `:974-979`).
        delete this.liveWornBundlesByChar[characterId];
      } catch (err) {
        this.toasts.showError(err instanceof Error ? err.message : 'Failed to update outfit');
        allOk = false;
      }
    }
    // A committed outfit may have put garments on — every wear tally is
    // stale. v4 invalidates `queryKeys.wardrobe.all`; v5's lists are
    // signal-held, so the equivalent is the lists' reload + the outfit's.
    void this.data.reload();
    await this.outfit.refreshOutfit();
    return allOk;
  }

  // ---------------------------------------------------------------------------
  // The History tab's needs (P4.D318 — v4 `8d8618819` `useStagedLiveOutfits`
  // `isDirty` / `discard`, ported onto the INLINE staging above)
  // ---------------------------------------------------------------------------

  /**
   * True when a character has staged edits Done would commit — v4's `isDirty`
   * (`useStagedLiveOutfits.ts:198-207`), NOT `classifyStagedOutfits` (which
   * answers what Done should commit, and differs for a staged character with
   * no baseline, a pending-only one, and an equal staging with a bundle claim).
   */
  protected isLiveDirty(cid: string): boolean {
    const staged = this.liveStagedByChar()[cid];
    if (!staged) return (this.pendingLiveMutators[cid]?.length ?? 0) > 0;
    const baseline = this.liveBaselineByChar[cid];
    if (!baseline) return true;
    return (
      !equippedSlotsEqual(staged, baseline) || (this.liveWornBundlesByChar[cid]?.length ?? 0) > 0
    );
  }

  /**
   * Throw away a character's staged edits — the History tab's rollback must
   * never be flushed over on Done. The Live tab re-seeds from the next worn
   * snapshot that arrives, or at once with `{ reseed: true }` (call that after
   * the rollback's refetch has settled). v4 `useStagedLiveOutfits.ts:209-230`.
   */
  protected discardLive(cid: string, options?: { reseed?: boolean }): void {
    const seedKey = `${cid}|${this.chatId() ?? 'no-chat'}`;
    if (options?.reseed) {
      // Only if nothing has re-seeded it since the staging was dropped.
      if (!this.liveSeededByChar.has(seedKey)) this.liveReseedTick.update((t) => t + 1);
      return;
    }
    this.liveSeededByChar.delete(seedKey);
    delete this.pendingLiveMutators[cid];
    delete this.liveBaselineByChar[cid];
    this.liveStagedByChar.update((prev) => {
      const next = { ...prev };
      delete next[cid];
      return next;
    });
    delete this.liveWornBundlesByChar[cid];
  }

  /**
   * The History tab's `requestConfirmation` (v4's async prop type) over this
   * dialog's synchronous native `window.confirm` — the recorded divergence at
   * the top of this file (R-2). A stable field, so the binding never changes.
   */
  protected readonly historyConfirm = (message: string): Promise<boolean> =>
    Promise.resolve(window.confirm(message));

  private readonly historyDiscards = new Map<
    string,
    (options?: { reseed?: boolean }) => Promise<void>
  >();

  /**
   * The History tab's `discardStagedEdits` for one character — v4's
   * `(options) => live.discard(selectedCharacterId, options)` (`:537-546`),
   * ONE cached closure per character so the binding is stable across change
   * detection. The closure carries its character, so a rollback settles the
   * character CLICKED even if the selection moves mid-flight (R-10).
   *
   * The reseed arm is v4's `onSettled` twin (R-H / R-5): v4 invalidates
   * `wardrobe.all` (the snapshot AND every wear tally) before re-seeding; here
   * the lists and the outfit are signal-held, so they re-read (the flush's own
   * tail) and THEN the staging re-seeds.
   */
  protected discardFor(cid: string): (options?: { reseed?: boolean }) => Promise<void> {
    let discard = this.historyDiscards.get(cid);
    if (!discard) {
      discard = async (options?: { reseed?: boolean }) => {
        if (options?.reseed) {
          void this.data.reload();
          await this.outfit.refreshOutfit();
          this.discardLive(cid, { reseed: true });
          return;
        }
        this.discardLive(cid);
      };
      this.historyDiscards.set(cid, discard);
    }
    return discard;
  }

  /** v4 `:352-356` — Done: flush, then close only when every commit stuck. */
  protected requestClose(): void {
    void (async () => {
      const ok = await this.flushStagedLiveOutfits();
      if (ok) this.closed.emit();
    })();
  }

  /**
   * v4 `useFittingRoom.wear` — wear the fitting-room composition: atomic
   * replace via `mode: 'set_all'`. On success the dialog closes — Aurora +
   * avatar gen fire immediately server-side.
   */
  protected async wearFitting(): Promise<void> {
    const characterId = this.selectedCharacterId();
    const chatId = this.chatId();
    if (!characterId || !chatId) return;
    try {
      await dispatchWardrobe(this.core, {
        type: 'chatEquip',
        chatId,
        ...buildSetAllEquipBody(characterId, this.fittingSlots(), this.fittingWornBundleIds),
      });
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to wear this outfit');
      return;
    }
    this.toasts.showSuccess('Worn!');
    void this.data.reload();
    await this.outfit.refreshOutfit();
    this.closed.emit();
  }

  // ---------------------------------------------------------------------------
  // Avatar generation (v4 `AvatarGenerationPane`; the pane is presentational
  // here — the strings are identical either side of v4's ownership move)
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

  /** What the Live tab actually paints (v4 `displaySlots`): staged slots once
   *  seeding captured a baseline, else the worn snapshot, else empty. */
  protected readonly liveDisplaySlots = computed<EquippedSlots>(() => {
    const characterId = this.selectedCharacterId();
    if (!characterId) return EMPTY_EQUIPPED_SLOTS as EquippedSlots;
    if (this.booted) this.outfit.equippedOutfit();
    return (
      this.liveStagedByChar()[characterId] ??
      this.wornSlotsFor(characterId) ??
      (EMPTY_EQUIPPED_SLOTS as EquippedSlots)
    );
  });

  /** The stable `general:` option value (v4 `encodeWardrobeContainer`). */
  protected readonly generalContainerValue = encodeWardrobeContainer(GENERAL_CONTAINER);

  protected encodeContainer(container: WardrobeContainer): string {
    return encodeWardrobeContainer(container);
  }

  /** The `<select>`'s options load asynchronously, so the selection rides
   *  `[selected]` per option, never `[value]` (the standing dogfood-#6 rule). */
  protected isSelectedContainer(scope: WardrobeContainer['scope'], id: string | null): boolean {
    const current = this.selectedContainer();
    return current?.scope === scope && (current.id ?? null) === id;
  }

  /** v4 `:1077` — a mangled option value decodes to null, which parks the
   *  dialog on "Select a wardrobe to browse" rather than guessing. */
  protected pickContainer(value: string): void {
    this.selectedContainer.set(decodeWardrobeContainer(value));
  }

  // ---------------------------------------------------------------------------
  // Close plumbing
  // ---------------------------------------------------------------------------
  protected onBackdrop(): void {
    if (this.overlayOpen()) return;
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
    // An overlay (the Move-to-folder picker included — bug 229) owns this
    // Escape; it closes itself.
    if (this.overlayOpen()) return;
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
