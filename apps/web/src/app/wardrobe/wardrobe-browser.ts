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
import { RealtimeService } from '../core/realtime.service';
import { refreshUnlessRealtime } from '../item-browser/refresh-unless-realtime';
import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import {
  BrowserTogglesAfter,
  FolderBrowserShell,
  type BrowserFolderFiling,
  type BrowserToggles,
} from '../item-browser/folder-browser-shell';
import {
  createFolderBrowserState,
  createItemFolderHandlers,
  pluralize,
  type ItemFolderOps,
} from '../item-browser/folder-browser-state';
import { ToastService } from '../ui/toast.service';
import { isComposite } from './equipped-slots';
import { viewTierOf } from './folder-view';
import { folderCrumb, isSameFolder } from './folders';
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
import { PronounAwareToggle } from './pronoun-aware-toggle';
import type { PronounGuidanceFilter } from './pronoun-guidance-filter';
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


/**
 * The wardrobe browser — one container's closet as a small file manager: a
 * breadcrumb, the subfolders of the folder you are in, then its items, with
 * the search, kind, slot, sort and visibility filters, row actions, a select
 * mode for filing several items at once, and a "+ New Item" button. A port of
 * v4 `components/wardrobe/WardrobeBrowser.tsx` at the pin `624d0d25c`
 * (`4a38222e6`, `97f0d8cb8`'s picker state; `b2b783b47` onto the shared
 * `createFolderBrowserState` / `qt-folder-browser-shell`, so this file adds
 * only the wardrobe's filters, rows and wording; `94f041764`'s `aria-pressed`
 * on the slot filters).
 *
 * Folders are the operator's filing (an item's folder is its path under
 * `Wardrobe/`); they never change what a character can wear. In a character's
 * container the root is the character's own closet, and each shared wardrobe
 * they borrow from is one tier row after the own subfolders — entering it
 * shows that wardrobe's own tree. Folders are never merged across tiers. The
 * search box looks everywhere and shows each hit's folder.
 *
 * Mounted by the Wardrobe dialog (left column, any container) and by the
 * Prospero project page's Wardrobe card (that project's container).
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
  imports: [FolderBrowserShell, BrowserTogglesAfter, WardrobeFolderPickerDialog, WardrobeItemRow, PronounAwareToggle],
  template: `
    <qt-folder-browser-shell
      [state]="state"
      [hasContainer]="container() !== null"
      [loading]="data().loading()"
      testIdPrefix="wardrobe"
      rootLabel="Wardrobe"
      [scrollList]="scrollList()"
      [folderHandlers]="folderHandlers"
      [text]="text"
      [toggles]="toggles()"
      [itemTemplate]="row"
      [folderFiling]="folderFiling"
    >
      <input
        qtBrowserFilters
        type="search"
        [value]="titleFilter()"
        (input)="titleFilter.set($any($event.target).value)"
        placeholder="Search wardrobe…"
        class="qt-input qt-input-sm"
        aria-label="Search wardrobe by title"
      />
      <div qtBrowserFilters class="flex flex-wrap items-center justify-between gap-2">
        <div role="tablist" aria-label="Item kind" class="inline-flex gap-1 qt-bg-muted/50 rounded-lg p-1 self-start">
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
          <select class="qt-select qt-select-sm" aria-label="Sort wardrobe" (change)="listSort.set($any($event.target).value)">
            @for (s of listSorts; track s.value) {
              <option [value]="s.value" [selected]="listSort() === s.value">
                {{ s.label }}
              </option>
            }
          </select>
        </label>
      </div>
      <div qtBrowserFilters class="flex flex-wrap gap-1">
        @for (slot of slotFilters; track slot) {
          <button
            type="button"
            (click)="slotFilter.set(slot)"
            [attr.aria-pressed]="slotFilter() === slot"
            class="qt-button-sm"
            [class.qt-button-secondary]="slotFilter() === slot"
            [class.qt-button-ghost]="slotFilter() !== slot"
          >
            {{ slot === 'all' ? 'All' : slotMeta[slot].label }}
          </button>
        }
      </div>
      <label qtBrowserTogglesAfter class="flex items-center gap-2 qt-text-xs qt-text-secondary">
        <input
          type="checkbox"
          class="qt-checkbox"
          [checked]="neverWornOnly()"
          (change)="neverWornOnly.set($any($event.target).checked)"
        />
        Never worn
      </label>
      @if (isCharacterScope() && pronounFilter(); as filter) {
        <qt-pronoun-aware-toggle qtBrowserTogglesAfter [filter]="filter" [items]="data().listItems()" />
      }

      <!-- Sticky create controls. (v4's Import-from-image entry is the
           standing analyze-image deferral — no dead button.) -->
      <button qtBrowserFooter type="button" class="qt-button-primary qt-button-sm" [disabled]="!container()" (click)="newItem()">
        + New Item
      </button>

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
    </qt-folder-browser-shell>

    <ng-template #row let-item>
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
    </ng-template>
  `,
})
export class WardrobeBrowser {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly realtime = inject(RealtimeService);

  readonly container = input.required<WardrobeContainer | null>();
  readonly data = input.required<WardrobeListData>();
  readonly actions = input.required<WardrobeItemActions>();
  readonly showArchived = input(false);
  /**
   * Character view only: the pronoun-aware filter — owned by the caller so the
   * composers beside the list share its switch (v4 `eb90d9ed8`,
   * `WardrobeBrowser.tsx:100-102`).
   */
  readonly pronounFilter = input<PronounGuidanceFilter | null>(null);
  readonly wear = input<WardrobeRowWearControls | undefined>(undefined);
  /** Cap the list's height (the dialog) instead of letting it grow (a card). */
  readonly scrollList = input(true);
  readonly showArchivedChange = output<boolean>();

  protected readonly itemKinds: ItemKind[] = ['items', 'outfits'];
  protected readonly slotFilters = SLOT_FILTERS;
  protected readonly slotMeta = WARDROBE_SLOT_META;
  protected readonly listSorts = WARDROBE_LIST_SORTS;
  /** v4 pin `:241-245`. */
  protected readonly text = {
    noContainer: 'Select a wardrobe to browse.',
    folderEmpty: 'Nothing hangs here yet.',
    noMatch: 'No items match this filter.',
  };

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
  protected readonly working = signal(false);

  private readonly folders = createWardrobeFoldersStore(this.core);

  protected readonly isCharacterScope = computed(() => this.container()?.scope === 'character');

  constructor() {
    // v4 `useWardrobeFolders(container)`, re-read on every actions refresh
    // (v4's `folderLists` invalidation — "an item's folder may be new").
    effect(() => {
      const container = this.container();
      this.actions().refreshTick();
      untracked(() => void this.folders.load(container));
    });
  }

  /** The view tier of an item (v4 `:155-158`). */
  private readonly tierOf = computed(() => {
    const canManage = this.data().canManage;
    const characterView = this.isCharacterScope();
    return (item: WardrobeItemDto): string => viewTierOf(item, canManage(item), characterView);
  });

  /**
   * Pronoun guidance narrows the pool itself (character view, switch on), so
   * tier and folder counts agree with the rows beneath them (v4 `eb90d9ed8`,
   * `WardrobeBrowser.tsx:160-165`).
   */
  protected readonly offeredItems = computed(() => {
    const listItems = this.data().listItems();
    const filter = this.pronounFilter();
    return this.isCharacterScope() && filter ? filter.offer(listItems) : listItems;
  });

  protected readonly filteredItems = computed(() => {
    const listItems = this.offeredItems();
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

  /** v4 `:194-203`. */
  protected readonly state = createFolderBrowserState<WardrobeItemDto>({
    containerKey: () => {
      const c = this.container();
      return c ? encodeWardrobeContainer(c) : '';
    },
    items: this.offeredItems,
    filtered: this.filteredItems,
    tree: () => this.folders.folders(),
    tierOf: this.tierOf,
    characterView: this.isCharacterScope,
    showShared: this.showShared,
    searching: computed(() => this.titleFilter().trim().length > 0),
  });

  /** v4 `:206-230` — the wardrobe's folder wording; each write ends in `refreshLists()` unless realtime is up (R-J, H-U1). */
  protected readonly folderHandlers = createItemFolderHandlers<WardrobeItemDto>({
    state: this.state,
    ops: () => {
      const container = this.container();
      if (!container) return null;
      // v4 `600c2906f` (`use-item-folders.ts` `onSettled: refreshUnlessRealtime`):
      // the server publishes from every wardrobe folder write, so while the
      // socket is up that publish does the re-read.
      const settled = <T>(p: Promise<T>): Promise<T> =>
        p.finally(() => void refreshUnlessRealtime(this.realtime, [() => this.actions().refreshLists()]));
      const ops: ItemFolderOps = {
        create: (path) => settled(createWardrobeFolder(this.core, container, path)),
        rename: (path, name) => settled(renameWardrobeFolder(this.core, container, path, name)),
        remove: (path) => settled(deleteWardrobeFolder(this.core, container, path)),
      };
      return ops;
    },
    wording: {
      noun: 'item',
      rootPhrase: 'the top of the wardrobe',
      created: (name) => `The folder "${name}" is ready for hanging things in`,
      movedUp: (count) => `Folder removed; ${count} moved up a peg`,
      // The list holds archived pieces only when "Show archived" is ticked,
      // but the server moves them all; say so rather than understate the move.
      deleteMessage: ({ name, count, parentLabel }) => {
        const archivedNote = this.showArchived() ? '' : ' (and any archived pieces filed there)';
        return count > 0
          ? `Send its ${pluralize(count, 'item')}${archivedNote} up to ${parentLabel}? Only the folder itself goes.`
          : this.showArchived()
            ? `Remove the empty folder "${name}"?`
            : `Remove the folder "${name}"? Anything archived in it goes up to ${parentLabel}.`;
      },
    },
    confirm: (message) => this.actions().confirm(message),
    describeError: (error, fallback) => (error instanceof Error ? error.message : fallback),
  });

  /** v4 pin `:308-330` — Show archived, Show shared (character view only); the after slot is the template's. */
  protected readonly toggles = computed<BrowserToggles>(() => ({
    archived: { checked: this.showArchived(), onChange: (next) => this.showArchivedChange.emit(next) },
    // Only the character view merges in other tiers; browsing a container,
    // every row already belongs to it.
    shared: this.isCharacterScope()
      ? { checked: this.showShared(), onChange: (next) => this.showShared.set(next) }
      : undefined,
  }));

  /** v4 `folderFiling={{ open: actions.openFolderPicker }}`. */
  protected readonly folderFiling: BrowserFolderFiling<WardrobeItemDto> = {
    open: (items, mode) => this.openPickerFor(items, mode),
  };

  protected newItem(): void {
    this.actions().openCreate('single', [], {
      folder: this.state.inOwnTier() ? this.state.currentFolder() : '',
    });
  }

  /**
   * The row's `selection` input (C3 §5) — `undefined` outside select mode.
   * Memoised per row so a binding reads the same object across change-detection
   * passes (a fresh literal each pass trips the dev-mode `NG0100` check).
   */
  private readonly selections = computed(() => {
    if (!this.state.selectMode()) return null;
    const ids = this.state.selectedIds();
    return new Map(
      this.state.visible().map((item) => [
        item.id,
        { selected: ids.has(item.id), onToggle: () => this.state.toggleSelected(item) },
      ]),
    );
  });

  protected selectionFor(item: WardrobeItemDto): { selected: boolean; onToggle: () => void } | undefined {
    return this.selections()?.get(item.id);
  }

  /** The row's `folderCrumb` input (C3 §5) — only while searching. */
  protected crumbFor(item: WardrobeItemDto): string | null {
    return this.state.searching() && item.folder ? folderCrumb(item.folder) : null;
  }

  /**
   * The row's **Move to folder…** / **Copy to folder…** and the select bar's
   * (v4 `97f0d8cb8`; the copy `bbbcd2388`).
   */
  protected openPickerFor(items: WardrobeItemDto[], mode: 'move' | 'copy'): void {
    this.actions().openFolderPicker(items, mode);
  }

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
    return items.length === 1 ? `"${items[0].title}"` : pluralize(items.length, 'item');
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
            : `Copies of ${pluralize(filed.length, 'item')} now hang in ${where}`
          : items.length === 1
            ? `"${items[0].title}" now hangs in ${where}`
            : `${pluralize(filed.length, 'item')} now hang in ${where}`,
      );
      this.actions().closeFolderPicker();
      this.state.setSelectMode(false);
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
