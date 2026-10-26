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
import { ItemFolderBar, NewFolderInput } from '../item-browser/item-folder-bar';
import { ItemFolderRow } from '../item-browser/item-folder-row';
import { ItemSelectBar } from '../item-browser/item-select-bar';
import { ItemTierRow } from '../item-browser/item-tier-row';
import { ToastService } from '../ui/toast.service';
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
  isSameFolder,
  isUnderFolder,
  joinFolder,
  parentFolder,
} from './folders';
import { WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES } from './slot-meta';
import { encodeWardrobeContainer, type WardrobeContainer } from './wardrobe-container';
import { WardrobeFolderPickerDialog } from './wardrobe-folder-picker-dialog';
import {
  copyWardrobeItemsToFolder,
  createWardrobeFolder,
  createWardrobeFoldersStore,
  deleteWardrobeFolder,
  moveWardrobeItemsToFolder,
  renameWardrobeFolder,
  type WardrobeFolderComponentMode,
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
 */
@Component({
  selector: 'qt-wardrobe-browser',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    ItemFolderBar,
    NewFolderInput,
    ItemSelectBar,
    ItemTierRow,
    ItemFolderRow,
    WardrobeFolderPickerDialog,
    WardrobeItemRow,
  ],
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

        <!-- Folder bar: where we are, and the filing controls (the shared
             item-browser bar since v4 4fbdad186 — P4.D330 R-X). -->
        @if (container()) {
          <qt-item-folder-bar
            rootLabel="Wardrobe"
            navLabel="Wardrobe folders"
            [searching]="searching()"
            [tierLabel]="inOwnTier() ? null : currentTierLabel()"
            [currentFolder]="currentFolder()"
            (navigateRoot)="navigate(ownTier, '')"
            (navigateTier)="navigate(currentTier(), '')"
            (navigateFolder)="navigate(currentTier(), $event)"
            [managed]="managed()"
            [newFolderOpen]="newFolderOpen()"
            (newFolder)="setNewFolderName('')"
            [selectMode]="selectMode()"
            (toggleSelect)="toggleSelectMode()"
            testId="wardrobe-folder-bar"
          />
        }
        @if (newFolderOpen() && managed() && !searching()) {
          <qt-new-folder-input
            [name]="newFolderName()"
            (nameChange)="setNewFolderName($event)"
            (submitted)="submitNewFolder()"
            (cancelled)="closeNewFolder()"
          />
        }
      </div>

      <div [class]="listClass()">
        @if (!container()) {
          <div class="text-sm qt-text-secondary px-3 py-4">Select a wardrobe to browse.</div>
        } @else if (data().loading()) {
          <div class="text-sm qt-text-secondary px-3 py-4">Loading…</div>
        } @else {
          @for (row of folderRows(); track row.path) {
            <!-- The shared item-browser row under the wardrobe's own element
                 name (v4 4fbdad186 — P4.D330 R-X). -->
            <qt-wardrobe-folder-row
              [row]="row"
              [manageable]="managed()"
              [rename]="renameFolderFor(row.path)"
              (opened)="navigate(currentTier(), row.path)"
              (deleted)="deleteFolder(row.path)"
              testId="wardrobe-folder-row"
              countNoun="item"
            />
          }
          @for (row of sharedTierRows(); track row.key) {
            <qt-item-tier-row
              [label]="row.label"
              [scope]="row.scope"
              [itemCount]="row.itemCount"
              (opened)="navigate(row.key, '')"
              testId="wardrobe-tier-row"
            />
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
                [selection]="selectionFor(item)"
                [folderCrumb]="crumbFor(item)"
                [canMoveToFolder]="true"
                (moveToFolder)="openPickerFor([$event], 'move')"
                [canCopyToFolder]="true"
                (copyToFolder)="openPickerFor([$event], 'copy')"
              />
            }
          }
          @if (selectMode()) {
            <qt-item-select-bar
              [count]="selectedIds().size"
              [sticky]="scrollList()"
              testId="wardrobe-select-bar"
              (cancelled)="setSelectMode(false); clearSelection()"
            >
              <button
                type="button"
                class="qt-button-secondary qt-button-sm"
                [disabled]="selectedItems().length === 0"
                (click)="openPickerFor(selectedItems(), 'move')"
              >
                Move to folder…
              </button>
              <button
                type="button"
                class="qt-button-secondary qt-button-sm"
                [disabled]="selectedItems().length === 0"
                (click)="openPickerFor(selectedItems(), 'copy')"
              >
                Copy to folder…
              </button>
            </qt-item-select-bar>
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
            [mode]="picker.mode"
            [container]="home"
            [subject]="pickerSubject()"
            [outfits]="pickerOutfits()"
            [currentFolder]="pickerCurrentFolder()"
            [working]="working()"
            (closed)="actions().closeFolderPicker()"
            (submitted)="fileToFolder($event)"
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
  protected readonly working = signal(false);

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
    // Read BEFORE the flip (v4's handler closes over the render's
    // `inOwnTier`): once hidden, a borrowed tier already reads as the root,
    // so asking afterwards would never navigate — and re-ticking would drop
    // the operator back inside the tier they left.
    const wasInOwnTier = this.inOwnTier();
    this.showShared.set(checked);
    // Hiding shared wardrobes from inside one goes back to the closet.
    if (!checked && !wasInOwnTier) this.navigate(OWN_TIER, '');
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

  /** v4 `:583` — the dialog caps the list's height; a card lets it grow. */
  protected readonly listClass = computed(
    () => `flex-1 space-y-1 pb-12 ${this.scrollList() ? 'overflow-y-auto max-h-[55vh]' : ''}`,
  );

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

  /** The row's `folderCrumb` input (C3 §5) — only while searching. */
  protected crumbFor(item: WardrobeItemDto): string | null {
    return this.searching() && item.folder ? folderCrumb(item.folder) : null;
  }

  /**
   * The row's **Move to folder…** / **Copy to folder…** and the select bar's
   * (v4 `97f0d8cb8`; the copy `bbbcd2388`).
   */
  protected openPickerFor(items: WardrobeItemDto[], mode: 'move' | 'copy'): void {
    this.actions().openFolderPicker(items, mode);
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

  /** The outfits among the items being filed — the picker asks after their components. */
  protected readonly pickerOutfits = computed(
    () => this.actions().folderPicker()?.items.filter(isComposite) ?? [],
  );

  /**
   * v4 `fileToFolder(folder, components?)` (`bbbcd2388`) — one `move-items` /
   * `copy-items` call. The picker's mode and items are read into locals
   * BEFORE anything is set (closing the picker clears them).
   */
  protected async fileToFolder(choice: {
    folder: string;
    components?: WardrobeFolderComponentMode;
  }): Promise<void> {
    const picker = this.actions().folderPicker();
    const home = this.pickerHome();
    if (!picker || !home) return;
    const { folder, components } = choice;
    const copying = picker.mode === 'copy';
    const items = picker.items;
    this.working.set(true);
    try {
      const ids = items.map((i) => i.id);
      const filed = copying
        ? await copyWardrobeItemsToFolder(
            this.core,
            home,
            ids,
            folder,
            components === 'move' ? undefined : components,
          )
        : await moveWardrobeItemsToFolder(this.core, home, ids, folder, components);
      const where = folder
        ? `"${folderCrumb(filed[0]?.folder || folder)}"`
        : 'the top of the wardrobe';
      this.toasts.showSuccess(
        copying
          ? items.length === 1
            ? `A copy of "${items[0].title}" now hangs in ${where}`
            : `Copies of ${plural(filed.length, 'item')} now hang in ${where}`
          : items.length === 1
            ? `"${items[0].title}" now hangs in ${where}`
            : `${plural(filed.length, 'item')} now hang in ${where}`,
      );
      this.actions().closeFolderPicker();
      this.clearSelection();
      this.setSelectMode(false);
    } catch (error) {
      this.toasts.showError(
        error instanceof Error
          ? error.message
          : copying
            ? 'The items would not be copied'
            : 'The items would not be moved',
      );
    } finally {
      this.working.set(false);
      void this.actions().refreshLists();
    }
  }
}
