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

import { CoreClient } from '../core/core-client';
import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import { QtAutofocus } from './autofocus.directive';
import { isComposite } from './equipped-slots';
import {
  childFolderRows,
  OWN_TIER,
  tierLabel,
  tierRows,
  viewTierOf,
  type FolderRowView,
  type TierRowView,
} from './folder-view';
import {
  folderCrumb,
  folderName,
  folderSegments,
  isSameFolder,
  isUnderFolder,
  joinFolder,
  parentFolder,
} from './folders';
import { WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES } from './slot-meta';
import { encodeWardrobeContainer, type WardrobeContainer } from './wardrobe-container';
import { WardrobeFolderPickerDialog } from './wardrobe-folder-picker-dialog';
import {
  createWardrobeFolder,
  createWardrobeFoldersStore,
  deleteWardrobeFolder,
  moveWardrobeItemsToFolder,
  renameWardrobeFolder,
} from './wardrobe-folders.api';
import type { WardrobeItemActions } from './wardrobe-item-actions';
import { WardrobeItemRow } from './wardrobe-item-row';
import type { WardrobeListData } from './wardrobe-list-data';
import { sortAndFilterWardrobeItems, WARDROBE_LIST_SORTS, type WardrobeListSort } from './wear-display';

type SlotFilter = 'all' | WardrobeSlotType;
const SLOT_FILTERS: SlotFilter[] = ['all', ...WARDROBE_SLOT_TYPES];
type ItemKind = 'items' | 'outfits';

/** Wear controls on each row — only when a character is being dressed (v4 `:73-78`). */
export interface WardrobeRowWearControls {
  equipLabel: string;
  addAction: 'layer' | 'add';
  onEquip: (item: WardrobeItemDto) => void;
  onAddToSlot: (item: WardrobeItemDto, slot: WardrobeSlotType) => void;
}

/** v4 `:93`. */
const plural = (n: number, word: string): string => `${n} ${word}${n === 1 ? '' : 's'}`;
const EMPTY_SELECTION: ReadonlySet<string> = new Set();

/**
 * One subfolder in the browser: open it, or (managed) rename / delete it.
 * v4 `WardrobeBrowser.tsx:698-804` `WardrobeFolderRow`.
 */
@Component({
  selector: 'qt-wardrobe-folder-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, QtAutofocus],
  template: `
    <div
      class="qt-card-interactive py-2 px-3 flex items-center gap-2"
      data-testid="wardrobe-folder-row"
    >
      <qt-icon name="folder" class="w-4 h-4 qt-text-secondary flex-shrink-0" />
      @if (renaming()) {
        <input
          type="text"
          qtAutofocus
          [value]="name()"
          (input)="name.set($any($event.target).value)"
          (keydown.enter)="$event.preventDefault(); submit()"
          (keydown.escape)="$event.stopPropagation(); cancelRename()"
          (blur)="submit()"
          [attr.aria-label]="'Rename folder ' + row().name"
          class="qt-input qt-input-sm flex-1"
        />
      } @else {
        <button
          type="button"
          class="flex-1 min-w-0 text-left text-sm text-foreground truncate"
          (click)="opened.emit()"
        >
          {{ row().name }}
        </button>
      }
      <span class="qt-text-xs qt-text-secondary flex-shrink-0">{{
        plural(row().itemCount, 'item')
      }}</span>
      @if (manageable() && !renaming()) {
        <div class="relative">
          <button
            type="button"
            class="qt-button-ghost qt-button-sm"
            [attr.aria-label]="'Folder actions for ' + row().name"
            aria-haspopup="menu"
            [attr.aria-expanded]="menuOpen()"
            (click)="menuOpen.set(!menuOpen())"
          >
            ⋮
          </button>
          @if (menuOpen()) {
            <div
              role="menu"
              class="absolute right-0 top-full mt-1 z-30 min-w-[10rem] rounded border qt-border-default qt-bg-default shadow-md"
              (mouseleave)="menuOpen.set(false)"
            >
              <ul class="divide-y qt-border-default">
                <li>
                  <button
                    type="button"
                    role="menuitem"
                    class="block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted"
                    (click)="startRename()"
                  >
                    Rename
                  </button>
                </li>
                <li>
                  <button
                    type="button"
                    role="menuitem"
                    class="block w-full text-left px-3 py-2 text-sm qt-text-destructive hover:qt-bg-muted"
                    (click)="menuOpen.set(false); deleted.emit()"
                  >
                    Delete
                  </button>
                </li>
              </ul>
            </div>
          }
        </div>
      }
    </div>
  `,
})
export class WardrobeFolderRow {
  readonly row = input.required<FolderRowView>();
  /** Rename / Delete are offered only in the wardrobe being browsed. */
  readonly manageable = input(false);
  /** Resolves true when the rename stuck (v4 `onRename`). */
  readonly rename = input.required<(name: string) => Promise<boolean>>();
  readonly opened = output<void>();
  readonly deleted = output<void>();

  protected readonly plural = plural;
  protected readonly menuOpen = signal(false);
  protected readonly renaming = signal(false);
  protected readonly name = signal('');
  /** One rename at a time: a blur landing while the Enter's call is out must not send it twice. */
  private submitting = false;

  protected startRename(): void {
    this.menuOpen.set(false);
    this.name.set(this.row().name);
    this.renaming.set(true);
  }

  protected cancelRename(): void {
    this.name.set(this.row().name);
    this.renaming.set(false);
  }

  /** v4 `:713-720`: blank or unchanged just stops; otherwise stop only on success. */
  protected async submit(): Promise<void> {
    if (!this.renaming() || this.submitting) return;
    const next = this.name().trim();
    if (!next || next === this.row().name) {
      this.renaming.set(false);
      return;
    }
    this.submitting = true;
    try {
      if (await this.rename()(next)) this.renaming.set(false);
    } finally {
      this.submitting = false;
    }
  }
}

/**
 * The wardrobe browser — one container's closet as a small file manager: a
 * breadcrumb, the subfolders of the folder you are in, then its items, with
 * the search, kind, slot, sort and visibility filters, row actions, a select
 * mode for filing several items at once, and a "+ New Item" button. A port of
 * v4 `components/wardrobe/WardrobeBrowser.tsx` at `4a38222e6` with
 * `97f0d8cb8`'s picker state (the picker reads `actions.folderPicker`).
 *
 * Folders are the operator's filing (an item's folder is its path under
 * `Wardrobe/`); they never change what a character can wear. In a character's
 * container the root is the character's own closet, and each shared wardrobe
 * they borrow from is one tier row after the own subfolders — entering it
 * shows that wardrobe's own tree. Folders are never merged across tiers. The
 * search box looks everywhere and shows each hit's folder.
 *
 * Mounted by the Wardrobe dialog (left column, any container) and by the
 * Prospero project page's Wardrobe card (that project's container). The data
 * comes from `createWardrobeListData`, the actions from
 * `createWardrobeItemActions`, the folder tree from a folders store, and the
 * view rules from `folder-view.ts`; this file lays them out.
 *
 * **View state is remembered per container, not reset by effects** (v4
 * `:123-134`): each piece remembers the container (or view) it was set in and
 * reads as its starting value anywhere else, so a new container opens at its
 * root with nothing selected, and leaving a folder or toggling search drops
 * the selection.
 *
 * **Recorded divergences.**
 *  - The "Import from image" footer button is not shipped (the standing
 *    `analyze-image` deferral, R-L) — v4 shows it only when a host passes
 *    `onImportFromImage`, and v5's hosts have none to pass.
 *  - v4's folder mutations invalidate every tree and list in `onSettled`;
 *    here each folder call ends in `actions.refreshLists()` (R-J).
 *  - The row's three new bindings — `[selection]`, `[folderCrumb]` and the
 *    **Move to folder…** output (C3 §5) — land with P4.D286's row and are
 *    this lane's HANDOFF to the unifier (status-log, P4.D285 unit 6);
 *    `selectionFor` / `crumbFor` / `openPickerFor` are their values here.
 */
@Component({
  selector: 'qt-wardrobe-browser',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, QtAutofocus, WardrobeFolderPickerDialog, WardrobeFolderRow, WardrobeItemRow],
  template: `
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
        <div class="flex flex-wrap items-center justify-between gap-2">
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
          <label class="flex items-center gap-2 qt-text-xs qt-text-secondary">
            Sort
            <select
              class="qt-select qt-select-sm"
              aria-label="Sort wardrobe"
              (change)="listSort.set($any($event.target).value)"
            >
              @for (s of listSorts; track s.value) {
                <option [value]="s.value" [selected]="listSort() === s.value">
                  {{ s.label }}
                </option>
              }
            </select>
          </label>
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
              {{ slot === 'all' ? 'All' : slotMeta[slot].label }}
            </button>
          }
        </div>
        <div class="flex flex-wrap items-center gap-x-4 gap-y-1">
          <label class="flex items-center gap-2 qt-text-xs qt-text-secondary">
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="showArchived()"
              (change)="showArchivedChange.emit($any($event.target).checked)"
            />
            Show archived
          </label>
          <!-- Only the character view merges in other tiers; browsing a
               container, every row already belongs to it. -->
          @if (isCharacterScope()) {
            <label class="flex items-center gap-2 qt-text-xs qt-text-secondary">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="showShared()"
                (change)="onShowShared($any($event.target).checked)"
              />
              Show shared
            </label>
          }
          <label class="flex items-center gap-2 qt-text-xs qt-text-secondary">
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="neverWornOnly()"
              (change)="neverWornOnly.set($any($event.target).checked)"
            />
            Never worn
          </label>
        </div>

        <!-- Folder bar: where we are, and the filing controls. -->
        @if (container()) {
          <div
            class="flex flex-wrap items-center justify-between gap-2"
            data-testid="wardrobe-folder-bar"
          >
            <nav
              aria-label="Wardrobe folders"
              class="flex flex-wrap items-center gap-1 qt-text-xs min-w-0"
            >
              @if (searching()) {
                <span class="qt-text-secondary">Searching everywhere</span>
              } @else {
                <button
                  type="button"
                  class="qt-text-secondary hover:text-foreground"
                  (click)="navigate(ownTier, '')"
                  [attr.aria-current]="inOwnTier() && currentFolder() === '' ? 'page' : null"
                >
                  Wardrobe
                </button>
                @if (!inOwnTier()) {
                  <span class="qt-text-secondary" aria-hidden="true">›</span>
                  <button
                    type="button"
                    class="qt-text-secondary hover:text-foreground"
                    (click)="navigate(currentTier(), '')"
                    [attr.aria-current]="currentFolder() === '' ? 'page' : null"
                  >
                    {{ currentTierLabel() }}
                  </button>
                }
                @for (crumb of crumbs(); track crumb.path; let last = $last) {
                  <span class="inline-flex items-center gap-1">
                    <span class="qt-text-secondary" aria-hidden="true">›</span>
                    <button
                      type="button"
                      [class]="last ? 'text-foreground font-medium' : 'qt-text-secondary hover:text-foreground'"
                      (click)="navigate(currentTier(), crumb.path)"
                      [attr.aria-current]="last ? 'page' : null"
                    >
                      {{ crumb.segment }}
                    </button>
                  </span>
                }
              }
            </nav>
            @if (managed()) {
              <div class="flex items-center gap-1">
                @if (!searching()) {
                  <button
                    type="button"
                    class="qt-button-ghost qt-button-sm"
                    (click)="setNewFolderName('')"
                    [disabled]="newFolderOpen()"
                  >
                    New folder
                  </button>
                }
                <button
                  type="button"
                  class="qt-button-sm"
                  [class.qt-button-secondary]="selectMode()"
                  [class.qt-button-ghost]="!selectMode()"
                  [attr.aria-pressed]="selectMode()"
                  (click)="toggleSelectMode()"
                >
                  Select
                </button>
              </div>
            }
          </div>
        }
        @if (newFolderOpen() && managed() && !searching()) {
          <div class="flex items-center gap-2">
            <input
              type="text"
              qtAutofocus
              [value]="newFolderName()"
              (input)="setNewFolderName($any($event.target).value)"
              (keydown.enter)="$event.preventDefault(); submitNewFolder()"
              (keydown.escape)="$event.stopPropagation(); closeNewFolder()"
              placeholder="Folder name — Enter to make it, Esc to think better of it"
              aria-label="New folder name"
              class="qt-input qt-input-sm flex-1"
            />
          </div>
        }
      </div>

      <div [class]="listClass()">
        @if (!container()) {
          <div class="text-sm qt-text-secondary px-3 py-4">Select a wardrobe to browse.</div>
        } @else if (data().loading()) {
          <div class="text-sm qt-text-secondary px-3 py-4">Loading…</div>
        } @else {
          @for (row of folderRows(); track row.path) {
            <qt-wardrobe-folder-row
              [row]="row"
              [manageable]="managed()"
              [rename]="renameFolderFor(row.path)"
              (opened)="navigate(currentTier(), row.path)"
              (deleted)="deleteFolder(row.path)"
            />
          }
          @for (row of sharedTierRows(); track row.key) {
            <button
              type="button"
              class="qt-card-interactive w-full py-2 px-3 flex items-center gap-2 text-left"
              (click)="navigate(row.key, '')"
              data-testid="wardrobe-tier-row"
            >
              <qt-icon name="folder" class="w-4 h-4 qt-text-secondary" />
              <span class="text-sm text-foreground flex-1 min-w-0 truncate">{{ row.label }}</span>
              <span class="qt-badge qt-badge-wardrobe-shared">{{ chipFor(row) }}</span>
              <span class="qt-text-xs qt-text-secondary">{{ plural(row.itemCount, 'item') }}</span>
            </button>
          }
          @if (folderIsEmpty()) {
            <div class="text-sm qt-text-secondary px-3 py-4">Nothing hangs here yet.</div>
          } @else if (
            visibleItems().length === 0 && folderRows().length === 0 && sharedTierRows().length === 0
          ) {
            <div class="text-sm qt-text-secondary px-3 py-4">No items match this filter.</div>
          } @else {
            @for (item of visibleItems(); track item.id) {
              <!-- Wear / +Layer show whenever a character is being dressed;
                   browsing a shared container there is nobody to dress. -->
              <qt-wardrobe-item-row
                [item]="item"
                [allItems]="data().resolutionPool()"
                [inChat]="!!wear()"
                [canManage]="data().canManage"
                [equipLabel]="wear()?.equipLabel ?? 'Wear'"
                [addAction]="wear()?.addAction ?? 'layer'"
                [isUpdatingDefault]="actions().updatingDefaultId() === item.id"
                canArchive
                canGenerateImage
                [generatingImageIds]="actions().generatingImageIds()"
                canOpenImage
                (toggleDefault)="actions().toggleDefault($event)"
                (toggleArchived)="actions().toggleArchived($event)"
                (generateImage)="actions().generateImage($event)"
                (openImage)="actions().openImage($event)"
                (edit)="actions().openEditor($event)"
                (duplicate)="actions().duplicate($event)"
                (move)="actions().move($event)"
                (copyItem)="actions().copy($event)"
                (delete)="actions().remove($event)"
                (equip)="wear()?.onEquip($event)"
                (addToSlot)="wear()?.onAddToSlot($event.item, $event.slot)"
              />
            }
          }
          @if (selectMode()) {
            <div [class]="selectBarClass()" data-testid="wardrobe-select-bar">
              <span class="qt-text-xs qt-text-secondary">{{ selectedIds().size }} selected</span>
              <div class="flex items-center gap-2">
                <button
                  type="button"
                  class="qt-button-secondary qt-button-sm"
                  [disabled]="selectedItems().length === 0"
                  (click)="openPickerFor(selectedItems())"
                >
                  Move to folder…
                </button>
                <button
                  type="button"
                  class="qt-button-ghost qt-button-sm"
                  (click)="setSelectMode(false); clearSelection()"
                >
                  Cancel
                </button>
              </div>
            </div>
          }
        }
      </div>

      <!-- Sticky create controls. (v4's Import-from-image entry is the
           standing analyze-image deferral — no dead button.) -->
      <div
        class="sticky bottom-0 -mx-1 px-1 pt-2 pb-1 qt-bg-default border-t qt-border-default flex items-center gap-2 justify-end"
      >
        <button
          type="button"
          class="qt-button-primary qt-button-sm"
          [disabled]="!container()"
          (click)="newItem()"
        >
          + New Item
        </button>
      </div>

      @if (actions().folderPicker(); as picker) {
        @if (pickerHome(); as home) {
          <qt-wardrobe-folder-picker-dialog
            [container]="home"
            [subject]="pickerSubject()"
            [currentFolder]="pickerCurrentFolder()"
            [moving]="moving()"
            (closed)="actions().closeFolderPicker()"
            (move)="moveToFolder($event)"
            (foldersChanged)="actions().refreshLists()"
          />
        }
      }
    </section>
  `,
})
export class WardrobeBrowser {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly container = input.required<WardrobeContainer | null>();
  readonly data = input.required<WardrobeListData>();
  readonly actions = input.required<WardrobeItemActions>();
  readonly showArchived = input(false);
  readonly wear = input<WardrobeRowWearControls | undefined>(undefined);
  /** Cap the list's height (the dialog) instead of letting it grow (a card). */
  readonly scrollList = input(true);
  readonly showArchivedChange = output<boolean>();

  protected readonly ownTier = OWN_TIER;
  protected readonly plural = plural;
  protected readonly itemKinds: ItemKind[] = ['items', 'outfits'];
  protected readonly slotFilters = SLOT_FILTERS;
  protected readonly slotMeta = WARDROBE_SLOT_META;
  protected readonly listSorts = WARDROBE_LIST_SORTS;

  /**
   * "Show shared": on by default, a purely client-side filter. The character
   * view's list *is* the merge of the character's own garments with every
   * shared tier above them, so hiding shared items means dropping the
   * un-manageable rows (and their tier rows) after the merge, not asking the
   * server for less. Never persisted.
   */
  protected readonly showShared = signal(true);
  protected readonly listSort = signal<WardrobeListSort>('title');
  protected readonly neverWornOnly = signal(false);
  protected readonly slotFilter = signal<SlotFilter>('all');
  protected readonly kindFilter = signal<ItemKind>('items');
  protected readonly titleFilter = signal('');

  // Where in the tree we are, remembered per container (v4 `:123-134`).
  private readonly view = signal({ key: '', tier: OWN_TIER, folder: '' });
  private readonly selectModeState = signal({ key: '', on: false });
  private readonly newFolderState = signal({ key: '', name: '' });
  private readonly selection = signal<{ key: string; ids: ReadonlySet<string> }>({
    key: '',
    ids: EMPTY_SELECTION,
  });
  protected readonly moving = signal(false);

  private readonly folders = createWardrobeFoldersStore(this.core);

  protected readonly isCharacterScope = computed(() => this.container()?.scope === 'character');
  private readonly containerKey = computed(() => {
    const c = this.container();
    return c ? encodeWardrobeContainer(c) : '';
  });
  protected readonly searching = computed(() => this.titleFilter().trim().length > 0);
  // Hiding shared wardrobes while inside one reads as the closet's root.
  private readonly viewIsCurrent = computed(() => {
    const v = this.view();
    return v.key === this.containerKey() && (this.showShared() || v.tier === OWN_TIER);
  });
  protected readonly currentTier = computed(() => (this.viewIsCurrent() ? this.view().tier : OWN_TIER));
  protected readonly currentFolder = computed(() => (this.viewIsCurrent() ? this.view().folder : ''));
  protected readonly inOwnTier = computed(() => this.currentTier() === OWN_TIER);
  protected readonly selectMode = computed(() => {
    const s = this.selectModeState();
    return s.key === this.containerKey() && s.on;
  });
  private readonly viewKey = computed(
    () => `${this.containerKey()}|${this.currentTier()}|${this.currentFolder()}|${this.searching()}`,
  );
  protected readonly selectedIds = computed<ReadonlySet<string>>(() => {
    const s = this.selection();
    return s.key === this.viewKey() ? s.ids : EMPTY_SELECTION;
  });
  protected readonly newFolderOpen = computed(() => this.newFolderState().key === this.viewKey());
  protected readonly newFolderName = computed(() =>
    this.newFolderOpen() ? this.newFolderState().name : '',
  );
  // Folders are managed only in the wardrobe being browsed — never in a
  // wardrobe a character merely borrows from.
  protected readonly managed = computed(() => this.inOwnTier() && this.container() !== null);

  constructor() {
    // v4 `useWardrobeFolders(container)`, re-read on every actions refresh
    // (v4's `folderLists` invalidation — "an item's folder may be new").
    effect(() => {
      const container = this.container();
      this.actions().refreshTick();
      untracked(() => void this.folders.load(container));
    });
  }

  /** The view tier of an item (v4 `:169-172`). */
  private readonly tierOf = (item: WardrobeItemDto): string =>
    viewTierOf(item, this.data().canManage(item), this.isCharacterScope());

  protected navigate(tier: string, folder: string): void {
    this.view.set({ key: this.containerKey(), tier, folder });
  }

  protected setSelectMode(on: boolean): void {
    this.selectModeState.set({ key: this.containerKey(), on });
  }

  protected clearSelection(): void {
    this.selection.set({ key: this.viewKey(), ids: new Set() });
  }

  protected toggleSelectMode(): void {
    this.setSelectMode(!this.selectMode());
    this.clearSelection();
  }

  protected setNewFolderName(name: string): void {
    this.newFolderState.set({ key: this.viewKey(), name });
  }

  protected closeNewFolder(): void {
    this.newFolderState.set({ key: '', name: '' });
  }

  protected onShowShared(checked: boolean): void {
    this.showShared.set(checked);
    // Hiding shared wardrobes from inside one goes back to the closet.
    if (!checked && !this.inOwnTier()) this.navigate(OWN_TIER, '');
  }

  protected readonly filteredItems = computed(() => {
    const listItems = this.data().listItems();
    const sorted = sortAndFilterWardrobeItems(listItems, {
      sort: this.listSort(),
      neverWornOnly: this.neverWornOnly(),
    });
    const term = this.titleFilter().trim().toLowerCase();
    const kindFilter = this.kindFilter();
    const slotFilter = this.slotFilter();
    const showShared = this.showShared();
    const characterScope = this.isCharacterScope();
    const canManage = this.data().canManage;
    return sorted.filter((i) => {
      // No archived filter here on purpose: the fetch already omitted them
      // unless "Show archived" is ticked. Shared items are the other way
      // round — the one rule for "is this shared" is `canManage`, the same
      // predicate that badges the row.
      if (characterScope && !showShared && !canManage(i)) return false;
      const composite = isComposite(i);
      if (kindFilter === 'items' && composite) return false;
      if (kindFilter === 'outfits' && !composite) return false;
      if (slotFilter !== 'all' && !i.types.includes(slotFilter)) return false;
      if (term && !i.title.toLowerCase().includes(term)) return false;
      return true;
    });
  });

  // Everything in the tier being browsed, filters aside — what folder rows
  // count and what "nothing hangs here" is judged against.
  private readonly tierItems = computed(() => {
    const tier = this.currentTier();
    return this.data()
      .listItems()
      .filter((i) => this.tierOf(i) === tier);
  });

  protected readonly folderRows = computed<FolderRowView[]>(() =>
    this.searching()
      ? []
      : childFolderRows(
          this.currentFolder(),
          this.tierItems(),
          this.managed() ? this.folders.folders() : [],
        ),
  );

  protected readonly sharedTierRows = computed<TierRowView[]>(() =>
    this.isCharacterScope() &&
    this.inOwnTier() &&
    !this.searching() &&
    this.currentFolder() === '' &&
    this.showShared()
      ? tierRows(this.data().listItems(), this.tierOf)
      : [],
  );

  protected readonly visibleItems = computed(() => {
    if (this.searching()) return this.filteredItems();
    const tier = this.currentTier();
    const folder = this.currentFolder();
    return this.filteredItems().filter(
      (i) => this.tierOf(i) === tier && isSameFolder(i.folder ?? '', folder),
    );
  });

  protected readonly currentTierLabel = computed(() => {
    if (this.inOwnTier()) return null;
    const tier = this.currentTier();
    const sample = this.data()
      .listItems()
      .find((i) => this.tierOf(i) === tier);
    return sample ? tierLabel(sample.origin) : 'Shared';
  });

  protected readonly folderIsEmpty = computed(
    () =>
      !this.searching() &&
      this.folderRows().length === 0 &&
      this.sharedTierRows().length === 0 &&
      !this.tierItems().some((i) => isSameFolder(i.folder ?? '', this.currentFolder())),
  );

  protected readonly crumbs = computed(() => {
    const segments = folderSegments(this.currentFolder());
    return segments.map((segment, idx) => ({
      segment,
      path: segments.slice(0, idx + 1).join('/'),
    }));
  });

  /** v4 `:583` — the dialog caps the list's height; a card lets it grow. */
  protected readonly listClass = computed(
    () => `flex-1 space-y-1 pb-12 ${this.scrollList() ? 'overflow-y-auto max-h-[55vh]' : ''}`,
  );

  /** v4 `:356` — sticky only inside the capped list. */
  protected readonly selectBarClass = computed(
    () =>
      `${this.scrollList() ? 'sticky bottom-0' : ''} mt-2 flex items-center justify-between gap-2 rounded border qt-border-default qt-bg-default px-3 py-2 shadow-sm`,
  );

  protected chipFor(row: TierRowView): string {
    return row.scope === 'general' ? 'Shared' : row.scope === 'project' ? 'Project' : 'Group';
  }

  protected newItem(): void {
    this.actions().openCreate('single', [], {
      folder: this.inOwnTier() ? this.currentFolder() : '',
    });
  }

  // ---- folder mutations (v4 `:249-301`) -------------------------------------

  protected async submitNewFolder(): Promise<void> {
    const name = this.newFolderName().trim();
    const container = this.container();
    if (!name || !container) return;
    try {
      const folder = await createWardrobeFolder(
        this.core,
        container,
        joinFolder(this.currentFolder(), name),
      );
      this.toasts.showSuccess(`The folder "${folder.name}" is ready for hanging things in`);
      this.closeNewFolder();
    } catch (error) {
      this.toasts.showError(error instanceof Error ? error.message : 'The folder would not be made');
    } finally {
      void this.actions().refreshLists();
    }
  }

  /** One stable rename function per folder path, for the folder row's input. */
  private readonly renameFns = new Map<string, (name: string) => Promise<boolean>>();
  protected renameFolderFor(path: string): (name: string) => Promise<boolean> {
    let fn = this.renameFns.get(path);
    if (!fn) {
      fn = (name: string) => this.renameFolder(path, name);
      this.renameFns.set(path, fn);
    }
    return fn;
  }

  private async renameFolder(path: string, name: string): Promise<boolean> {
    const container = this.container();
    if (!container) return false;
    try {
      const folder = await renameWardrobeFolder(this.core, container, path, name);
      this.toasts.showSuccess(`Renamed to "${folder.name}"`);
      return true;
    } catch (error) {
      this.toasts.showError(
        error instanceof Error ? error.message : 'The folder would not be renamed',
      );
      return false;
    } finally {
      void this.actions().refreshLists();
    }
  }

  protected async deleteFolder(path: string): Promise<void> {
    const container = this.container();
    if (!container) return;
    const parent = parentFolder(path);
    // The list holds archived pieces only when "Show archived" is ticked, but
    // the server moves them all; say so rather than understate the move.
    const count = this.tierItems().filter((i) => isUnderFolder(i.folder ?? '', path)).length;
    const archivedNote = this.showArchived() ? '' : ' (and any archived pieces filed there)';
    const parentLabel = parent ? `"${folderName(parent)}"` : 'the top of the wardrobe';
    const message =
      count > 0
        ? `Send its ${plural(count, 'item')}${archivedNote} up to ${parentLabel}? Only the folder itself goes.`
        : this.showArchived()
          ? `Remove the empty folder "${folderName(path)}"?`
          : `Remove the folder "${folderName(path)}"? Anything archived in it goes up to ${parentLabel}.`;
    if (!(await this.actions().confirm(message))) return;
    try {
      const result = await deleteWardrobeFolder(this.core, container, path);
      this.toasts.showSuccess(
        result.movedItems > 0
          ? `Folder removed; ${plural(result.movedItems, 'item')} moved up a peg`
          : 'Folder removed',
      );
      if (isUnderFolder(this.currentFolder(), path)) this.navigate(this.currentTier(), parent);
    } catch (error) {
      this.toasts.showError(
        error instanceof Error ? error.message : 'The folder would not be removed',
      );
    } finally {
      void this.actions().refreshLists();
    }
  }

  // ---- filing items (v4 `:303-381`) -----------------------------------------

  protected toggleSelected(item: WardrobeItemDto): void {
    const key = this.viewKey();
    this.selection.update((prev) => {
      const next = new Set(prev.key === key ? prev.ids : []);
      if (next.has(item.id)) next.delete(item.id);
      else next.add(item.id);
      return { key, ids: next };
    });
  }

  /**
   * The row's `selection` input (C3 §5) — `undefined` outside select mode.
   * Memoised per row so a binding reads the same object across change-detection
   * passes (a fresh literal each pass trips the dev-mode `NG0100` check).
   * HANDOFF: bound by the unifier once P4.D286's row has the input.
   */
  private readonly selections = computed(() => {
    if (!this.selectMode()) return null;
    const ids = this.selectedIds();
    return new Map(
      this.visibleItems().map((item) => [
        item.id,
        { selected: ids.has(item.id), onToggle: () => this.toggleSelected(item) },
      ]),
    );
  });

  protected selectionFor(
    item: WardrobeItemDto,
  ): { selected: boolean; onToggle: () => void } | undefined {
    return this.selections()?.get(item.id);
  }

  /** The row's `folderCrumb` input (C3 §5) — only while searching. HANDOFF, as above. */
  protected crumbFor(item: WardrobeItemDto): string | null {
    return this.searching() && item.folder ? folderCrumb(item.folder) : null;
  }

  /** The row's **Move to folder…** and the select bar's (v4 `97f0d8cb8`). */
  protected openPickerFor(items: WardrobeItemDto[]): void {
    this.actions().openFolderPicker(items);
  }

  protected readonly selectedItems = computed(() => {
    const ids = this.selectedIds();
    return this.data()
      .listItems()
      .filter((i) => ids.has(i.id));
  });

  protected readonly pickerHome = computed(() => {
    const picker = this.actions().folderPicker();
    return picker ? this.actions().addressOf(picker.items[0]) : null;
  });

  protected readonly pickerCurrentFolder = computed(() => {
    const picker = this.actions().folderPicker();
    if (!picker) return null;
    const first = picker.items[0].folder ?? '';
    return picker.items.every((i) => isSameFolder(i.folder ?? '', first)) ? first : null;
  });

  protected readonly pickerSubject = computed(() => {
    const items = this.actions().folderPicker()?.items ?? [];
    return items.length === 1 ? `"${items[0].title}"` : plural(items.length, 'item');
  });

  protected async moveToFolder(folder: string): Promise<void> {
    const picker = this.actions().folderPicker();
    const home = this.pickerHome();
    if (!picker || !home) return;
    this.moving.set(true);
    try {
      const moved = await moveWardrobeItemsToFolder(
        this.core,
        home,
        picker.items.map((i) => i.id),
        folder,
      );
      const where = folder
        ? `"${folderCrumb(moved[0]?.folder || folder)}"`
        : 'the top of the wardrobe';
      this.toasts.showSuccess(
        picker.items.length === 1
          ? `"${picker.items[0].title}" now hangs in ${where}`
          : `${plural(moved.length, 'item')} now hang in ${where}`,
      );
      this.actions().closeFolderPicker();
      this.clearSelection();
      this.setSelectMode(false);
    } catch (error) {
      this.toasts.showError(error instanceof Error ? error.message : 'The items would not be moved');
    } finally {
      this.moving.set(false);
      void this.actions().refreshLists();
    }
  }
}
