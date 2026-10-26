import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { CarriedEntry, ResolvedInventoryItem } from '../core/core-contract';
import { ItemFolderBar, NewFolderInput } from '../item-browser/item-folder-bar';
import { ItemFolderRow } from '../item-browser/item-folder-row';
import { ItemSelectBar } from '../item-browser/item-select-bar';
import { ItemTierRow } from '../item-browser/item-tier-row';
import { ToastService } from '../ui/toast.service';
import {
  OWN_TIER,
  childFolderRows,
  tierLabel,
  tierRows,
  type FolderRowView,
  type TierRowView,
} from '../wardrobe/folder-view';
import { folderCrumb, folderName, isSameFolder, isUnderFolder, joinFolder, parentFolder } from '../wardrobe/folders';
import { encodeWardrobeContainer } from '../wardrobe/wardrobe-container';
import { encodeInventoryContainer, type InventoryContainer } from './inventory-container';
import type { InventoryFoldersStore } from './inventory-folders-data';
import type { InventoryItemActions } from './inventory-item-actions';
import { InventoryItemRow, type InventoryRowActions, type InventoryRowEquip } from './inventory-item-row';
import type { InventoryListData } from './inventory-list-data';
import { describeInventoryRefusal, plural } from './inventory-refusal';
import { createInventoryFolder, deleteInventoryFolder, renameInventoryFolder } from './inventory.api';

/** v4 `:35`. */
export type KindFilter = 'items' | 'kits' | 'archetypes';
const KINDS: KindFilter[] = ['items', 'kits', 'archetypes'];
const KIND_LABEL: Record<KindFilter, string> = { items: 'Items', kits: 'Kits', archetypes: 'Archetypes' };

/** Equip controls on each row — only when a character in a chat is chosen (v4 `:38-42`). */
export interface InventoryBrowserEquip {
  carried: CarriedEntry;
  onEquip: (item: ResolvedInventoryItem) => void;
  onUnequip: (item: ResolvedInventoryItem) => void;
}

const EMPTY_SELECTION: ReadonlySet<string> = new Set();

/** The tier an object sits in from the character's view: own, or the tier it is borrowed from (v4 `:59-62`). */
function tierOfItem(item: ResolvedInventoryItem, manageable: boolean): string {
  if (manageable || !item.origin) return OWN_TIER;
  return encodeWardrobeContainer({
    scope: item.origin.scope,
    id: item.origin.scope === 'general' ? null : item.origin.id,
  });
}

/**
 * The Inventory browser — one tier's objects as a small file manager, a port
 * of v4 `components/inventory/InventoryBrowser.tsx` (`322a5488e`, with
 * `102a23648`'s tier-row fix), laid out like the wardrobe's and built from the
 * same `item-browser` pieces: a search box that looks everywhere, the *Items /
 * Kits / Archetypes* filter, *Equipped only* (in a chat), *Show archived* and
 * *Show shared*, the breadcrumb and folder rows, a character's borrowed tiers
 * as tier rows, select mode with *Move to folder…*, the rows, and the sticky
 * create buttons.
 *
 * Folders are the operator's filing; they never change what anyone carries.
 *
 * **View state is remembered per container, not reset by effects** (v4
 * `:80-101`, the wardrobe browser's idiom): each piece remembers the container
 * (or view) it was set in and reads as its starting value anywhere else.
 * `folderIsEmpty` counts ARCHIVED objects too (`tierItems` is unfiltered —
 * trap 10). The folder tree is the host's store (R-1: one `reload`).
 */
@Component({
  selector: 'qt-inventory-browser',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ItemFolderBar, NewFolderInput, ItemFolderRow, ItemSelectBar, ItemTierRow, InventoryItemRow],
  template: `
    <section class="flex flex-col min-h-0 relative">
      <div class="flex flex-col gap-2 mb-2">
        <input
          type="search"
          [value]="search()"
          (input)="search.set($any($event.target).value)"
          placeholder="Search the inventory…"
          class="qt-input qt-input-sm"
          aria-label="Search the inventory by name"
        />
        <div class="flex flex-wrap items-center gap-1" role="tablist" aria-label="Object kind">
          @for (k of kinds; track k) {
            <button
              type="button"
              role="tab"
              [attr.aria-selected]="kindFilter() === k"
              (click)="kindFilter.set(k)"
              [attr.class]="'qt-button-sm ' + (kindFilter() === k ? 'qt-button-secondary' : 'qt-button-ghost')"
            >
              {{ kindLabel[k] }}
            </button>
          }
        </div>
        <div class="flex flex-wrap items-center gap-x-4 gap-y-1">
          @if (equip()) {
            <label class="flex items-center gap-2 qt-text-xs qt-text-secondary">
              <input
                type="checkbox"
                [checked]="equippedOnly()"
                (change)="equippedOnly.set($any($event.target).checked)"
                class="qt-checkbox"
              />
              Equipped only
            </label>
          }
          <label class="flex items-center gap-2 qt-text-xs qt-text-secondary">
            <input
              type="checkbox"
              [checked]="showArchived()"
              (change)="showArchivedChange.emit($any($event.target).checked)"
              class="qt-checkbox"
            />
            Show archived
          </label>
          @if (isCharacterScope()) {
            <label class="flex items-center gap-2 qt-text-xs qt-text-secondary">
              <input
                type="checkbox"
                [checked]="showShared()"
                (change)="onShowShared($any($event.target).checked)"
                class="qt-checkbox"
              />
              Show shared
            </label>
          }
        </div>
        @if (container()) {
          <qt-item-folder-bar
            rootLabel="Inventory"
            navLabel="Inventory folders"
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
            (toggleSelect)="setSelectMode(!selectMode()); clearSelection()"
            testId="inventory-folder-bar"
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

      <div [attr.class]="listClass()">
        @if (!container()) {
          <div class="text-sm qt-text-secondary px-3 py-4">Select an inventory to browse.</div>
        } @else if (data().loading()) {
          <div class="text-sm qt-text-secondary px-3 py-4">Loading…</div>
        } @else {
          @for (row of folderRows(); track row.path) {
            <qt-item-folder-row
              [row]="row"
              [manageable]="managed()"
              (opened)="navigate(currentTier(), row.path)"
              [rename]="renameFolderFor(row.path)"
              (deleted)="deleteFolder(row.path)"
              testId="inventory-folder-row"
              countNoun="object"
            />
          }
          @for (row of sharedTierRows(); track row.key) {
            <qt-item-tier-row
              [label]="row.label"
              [scope]="row.scope"
              [itemCount]="row.itemCount"
              (opened)="navigate(row.key, '')"
              testId="inventory-tier-row"
              countNoun="object"
            />
          }
          @if (folderIsEmpty()) {
            <div class="text-sm qt-text-secondary px-3 py-4">Nothing on this shelf yet.</div>
          } @else if (visible().length === 0 && folderRows().length === 0 && sharedTierRows().length === 0) {
            <div class="text-sm qt-text-secondary px-3 py-4">Nothing matches this filter.</div>
          } @else {
            @for (item of visible(); track item.id) {
              <qt-inventory-item-row
                [item]="item"
                [manageable]="data().canManage(item)"
                [actions]="rowActions()"
                [equip]="rowEquipFor(item)"
                [selection]="selectionFor(item)"
                [folderCrumb]="searching() && item.folder ? crumb(item.folder) : null"
              />
            }
          }
          @if (selectMode()) {
            <qt-item-select-bar
              [count]="selectedIds().size"
              [sticky]="scrollList()"
              testId="inventory-select-bar"
              (cancelled)="setSelectMode(false); clearSelection()"
            >
              <button
                type="button"
                class="qt-button-secondary qt-button-sm"
                [disabled]="selectedItems().length === 0"
                (click)="actions().openFolderPicker(selectedItems())"
              >
                Move to folder…
              </button>
            </qt-item-select-bar>
          }
        }
      </div>

      <div
        class="sticky bottom-0 -mx-1 px-1 pt-2 pb-1 qt-bg-default border-t qt-border-default flex flex-wrap items-center gap-2 justify-end"
      >
        <button type="button" class="qt-button-ghost qt-button-sm" [disabled]="!container()" (click)="fromArchetype()">
          + From archetype
        </button>
        <button type="button" class="qt-button-ghost qt-button-sm" [disabled]="!container()" (click)="create('kit')">
          + New Kit
        </button>
        <button type="button" class="qt-button-ghost qt-button-sm" [disabled]="!container()" (click)="create('archetype')">
          + New Archetype
        </button>
        <button type="button" class="qt-button-primary qt-button-sm" [disabled]="!container()" (click)="create('item')">
          + New Item
        </button>
      </div>
    </section>
  `,
})
export class InventoryBrowser {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly container = input.required<InventoryContainer | null>();
  readonly data = input.required<InventoryListData>();
  readonly actions = input.required<InventoryItemActions>();
  /** The host's folder tree for `container` (R-1 — reloaded with the lists). */
  readonly folders = input.required<InventoryFoldersStore>();
  readonly showArchived = input(false);
  readonly equip = input<InventoryBrowserEquip | undefined>(undefined);
  /** Cap the list's height (the dialog) instead of letting it grow (a card). */
  readonly scrollList = input(true);
  readonly showArchivedChange = output<boolean>();

  protected readonly ownTier = OWN_TIER;
  protected readonly kinds = KINDS;
  protected readonly kindLabel = KIND_LABEL;
  protected readonly crumb = folderCrumb;

  protected readonly showShared = signal(true);
  protected readonly equippedOnly = signal(false);
  protected readonly kindFilter = signal<KindFilter>('items');
  protected readonly search = signal('');

  private readonly view = signal({ key: '', tier: OWN_TIER, folder: '' });
  private readonly selectModeState = signal({ key: '', on: false });
  private readonly newFolderState = signal({ key: '', name: '' });
  private readonly selection = signal<{ key: string; ids: ReadonlySet<string> }>({ key: '', ids: EMPTY_SELECTION });

  protected readonly isCharacterScope = computed(() => this.container()?.scope === 'character');
  private readonly containerKey = computed(() => {
    const c = this.container();
    return c ? encodeInventoryContainer(c) : '';
  });
  protected readonly searching = computed(() => this.search().trim().length > 0);
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
  protected readonly newFolderName = computed(() => (this.newFolderOpen() ? this.newFolderState().name : ''));
  protected readonly managed = computed(() => this.inOwnTier() && this.container() !== null);

  private readonly tierOf = (item: ResolvedInventoryItem): string => tierOfItem(item, this.data().canManage(item));

  private readonly carriedIds = computed(() => new Set((this.equip()?.carried.equipped ?? []).map((i) => i.id)));

  /** v4 `:110-125` — the filters, then the `base`-sensitivity sort by label. */
  protected readonly filtered = computed(() => {
    const term = this.search().trim().toLowerCase();
    const characterScope = this.isCharacterScope();
    const showShared = this.showShared();
    const canManage = this.data().canManage;
    const kind = this.kindFilter();
    const equippedOnly = this.equippedOnly();
    const equip = this.equip();
    const carried = this.carriedIds();
    return this.data()
      .listItems()
      .filter((item) => {
        if (characterScope && !showShared && !canManage(item)) return false;
        if (kind === 'kits' && item.kind !== 'kit') return false;
        if (kind === 'archetypes' && !item.isArchetype) return false;
        if (kind === 'items' && (item.kind === 'kit' || item.isArchetype)) return false;
        if (
          equippedOnly &&
          equip &&
          !carried.has(item.id) &&
          !(item.kind === 'kit' && equip.carried.kits[item.id] !== 'none')
        ) {
          return false;
        }
        if (term && !`${item.title ?? ''} ${item.name}`.toLowerCase().includes(term)) return false;
        return true;
      })
      .sort((a, b) => (a.title || a.name).localeCompare(b.title || b.name, undefined, { sensitivity: 'base' }));
  });

  /** Everything in the tier being browsed, filters (and archiving) aside — trap 10. */
  private readonly tierItems = computed(() => {
    const tier = this.currentTier();
    return this.data()
      .listItems()
      .filter((i) => this.tierOf(i) === tier);
  });

  protected readonly folderRows = computed<FolderRowView[]>(() =>
    this.searching()
      ? []
      : childFolderRows(this.currentFolder(), this.tierItems(), this.managed() ? this.folders().folders() : []),
  );

  /** v4 `:133` (`102a23648`) — tier rows from the FILTERED objects. */
  protected readonly sharedTierRows = computed<TierRowView[]>(() =>
    this.isCharacterScope() && this.inOwnTier() && !this.searching() && this.currentFolder() === '' && this.showShared()
      ? tierRows(this.filtered(), this.tierOf)
      : [],
  );

  protected readonly visible = computed(() => {
    if (this.searching()) return this.filtered();
    const tier = this.currentTier();
    const folder = this.currentFolder();
    return this.filtered().filter((i) => this.tierOf(i) === tier && isSameFolder(i.folder ?? '', folder));
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

  protected readonly listClass = computed(
    () => `flex-1 space-y-1 pb-12 ${this.scrollList() ? 'overflow-y-auto max-h-[55vh]' : ''}`,
  );

  protected readonly selectedItems = computed(() => {
    const ids = this.selectedIds();
    return this.data()
      .listItems()
      .filter((i) => ids.has(i.id));
  });

  /** v4 `:216-224` — one stable object for every row. */
  protected readonly rowActions = computed<InventoryRowActions>(() => {
    const actions = this.actions();
    return {
      onEdit: (item) => actions.openEditor(item),
      onInstantiate: (item) => actions.openInstantiateArchetype(item),
      onDetach: (item) => void actions.detach(item),
      onDuplicate: (item) => void actions.duplicate(item),
      onMoveToFolder: (item) => actions.openFolderPicker([item]),
      onTransfer: (item, mode) => actions.openTransfer(item, mode),
      onToggleArchived: (item) => void actions.toggleArchived(item),
      onDelete: (item) => void actions.remove(item),
    };
  });

  /** v4 `rowEquip` (`:205-214`), memoised per row (a fresh literal each pass trips NG0100). */
  private readonly rowEquips = computed(() => {
    const equip = this.equip();
    if (!equip) return null;
    const carried = this.carriedIds();
    return new Map<string, InventoryRowEquip>(
      this.visible().map((item) => [
        item.id,
        {
          equipped: carried.has(item.id),
          kitState: item.kind === 'kit' ? (equip.carried.kits[item.id] ?? 'none') : undefined,
          onEquip: equip.onEquip,
          onUnequip: equip.onUnequip,
        },
      ]),
    );
  });

  protected rowEquipFor(item: ResolvedInventoryItem): InventoryRowEquip | undefined {
    return this.rowEquips()?.get(item.id);
  }

  private readonly selections = computed(() => {
    if (!this.selectMode()) return null;
    const ids = this.selectedIds();
    return new Map(
      this.visible().map((item) => [
        item.id,
        { selected: ids.has(item.id), onToggle: (i: ResolvedInventoryItem) => this.toggleSelected(i) },
      ]),
    );
  });

  protected selectionFor(
    item: ResolvedInventoryItem,
  ): { selected: boolean; onToggle: (item: ResolvedInventoryItem) => void } | undefined {
    return this.selections()?.get(item.id);
  }

  protected navigate(tier: string, folder: string): void {
    this.view.set({ key: this.containerKey(), tier, folder });
  }

  protected setSelectMode(on: boolean): void {
    this.selectModeState.set({ key: this.containerKey(), on });
  }

  protected clearSelection(): void {
    this.selection.set({ key: this.viewKey(), ids: new Set() });
  }

  protected setNewFolderName(name: string): void {
    this.newFolderState.set({ key: this.viewKey(), name });
  }

  protected closeNewFolder(): void {
    this.newFolderState.set({ key: '', name: '' });
  }

  /**
   * v4 `:267-270`. Read `inOwnTier` BEFORE the flip (R-8 — v4's handler
   * closes over the render's value): once shared is hidden a borrowed tier
   * already reads as home, so asking afterwards would never navigate.
   */
  protected onShowShared(checked: boolean): void {
    const wasInOwnTier = this.inOwnTier();
    this.showShared.set(checked);
    if (!checked && !wasInOwnTier) this.navigate(OWN_TIER, '');
  }

  protected toggleSelected(item: ResolvedInventoryItem): void {
    const key = this.viewKey();
    this.selection.update((prev) => {
      const next = new Set(prev.key === key ? prev.ids : []);
      if (next.has(item.id)) next.delete(item.id);
      else next.add(item.id);
      return { key, ids: next };
    });
  }

  protected create(mode: 'item' | 'kit' | 'archetype'): void {
    this.actions().openCreate(mode, this.inOwnTier() ? this.currentFolder() : '');
  }

  protected fromArchetype(): void {
    const container = this.container();
    if (container) this.actions().openInstantiateInto(container, this.inOwnTier() ? this.currentFolder() : '');
  }

  // ---- folder writes (v4 `:150-190`), each refreshed afterwards (v4's onSettled) ----

  protected async submitNewFolder(): Promise<void> {
    const name = this.newFolderName().trim();
    const container = this.container();
    if (!name || !container) return;
    try {
      const folder = await createInventoryFolder(this.core, container, joinFolder(this.currentFolder(), name));
      this.toasts.showSuccess(`The folder "${folder.name}" is ready for the stock`);
      this.closeNewFolder();
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, 'The folder would not be made'));
    } finally {
      void this.actions().refresh();
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
      const folder = await renameInventoryFolder(this.core, container, path, name);
      this.toasts.showSuccess(`Renamed to "${folder.name}"`);
      return true;
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, 'The folder would not be renamed'));
      return false;
    } finally {
      void this.actions().refresh();
    }
  }

  protected async deleteFolder(path: string): Promise<void> {
    const container = this.container();
    if (!container) return;
    const parent = parentFolder(path);
    const count = this.tierItems().filter((i) => isUnderFolder(i.folder ?? '', path)).length;
    const parentLabel = parent ? `"${folderName(parent)}"` : 'the top of the inventory';
    const message =
      count > 0
        ? `Send its ${plural(count, 'object')} up to ${parentLabel}? Only the folder itself goes.`
        : `Remove the folder "${folderName(path)}"?`;
    if (!(await this.actions().requestConfirmation(message))) return;
    try {
      const result = await deleteInventoryFolder(this.core, container, path);
      this.toasts.showSuccess(
        result.movedItems > 0 ? `Folder removed; ${plural(result.movedItems, 'object')} moved up` : 'Folder removed',
      );
      if (isUnderFolder(this.currentFolder(), path)) this.navigate(this.currentTier(), parent);
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, 'The folder would not be removed'));
    } finally {
      void this.actions().refresh();
    }
  }
}
