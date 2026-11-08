import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { CoreClient } from '../core/core-client';
import { RealtimeService } from '../core/realtime.service';
import { refreshUnlessRealtime } from '../item-browser/refresh-unless-realtime';
import type { CarriedEntry, ResolvedInventoryItem } from '../core/core-contract';
import { BrowserTogglesBefore, FolderBrowserShell, type BrowserFolderFiling, type BrowserToggles } from '../item-browser/folder-browser-shell';
import {
  createFolderBrowserState,
  createItemFolderHandlers,
  type ItemFolderOps,
} from '../item-browser/folder-browser-state';
import { OWN_TIER } from '../item-browser/folder-view';
import { folderCrumb } from '../wardrobe/folders';
import { encodeWardrobeContainer } from '../wardrobe/wardrobe-container';
import { encodeInventoryContainer, type InventoryContainer } from './inventory-container';
import type { InventoryFoldersStore } from './inventory-folders-data';
import type { InventoryItemActions } from './inventory-item-actions';
import { InventoryItemRow, type InventoryRowActions, type InventoryRowEquip } from './inventory-item-row';
import type { InventoryListData } from './inventory-list-data';
import { describeInventoryRefusal } from './inventory-refusal';
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
 * of v4 `components/inventory/InventoryBrowser.tsx` at the pin `624d0d25c`
 * (`322a5488e`; `b2b783b47` onto the shared shell), laid out like the
 * wardrobe's and built from the same `item-browser` pieces: a search box that
 * looks everywhere, the *Items / Kits / Archetypes* filter, *Equipped only*
 * (in a chat), *Show archived* and *Show shared*, the breadcrumb and folder
 * rows, a character's borrowed tiers as tier rows, select mode with *Move /
 * Copy to folder…*, the rows, and the sticky create buttons. The view state
 * and layout are the shared `createFolderBrowserState` /
 * `qt-folder-browser-shell`; this file adds the Inventory's filters, rows and
 * wording.
 *
 * Folders are the operator's filing; they never change what anyone carries.
 * The folder tree is the host's store (R-1: one `reload`).
 *
 * **A measured shift at the pin:** v4's shared state counts tier rows from
 * the whole pool (`items`), where `102a23648`'s Inventory read them from the
 * FILTERED objects — the shell's rule now holds for both browsers.
 */
@Component({
  selector: 'qt-inventory-browser',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FolderBrowserShell, BrowserTogglesBefore, InventoryItemRow],
  template: `
    <qt-folder-browser-shell
      [state]="state"
      [hasContainer]="container() !== null"
      [loading]="data().loading()"
      testIdPrefix="inventory"
      rootLabel="Inventory"
      countNoun="object"
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
        [value]="search()"
        (input)="search.set($any($event.target).value)"
        placeholder="Search the inventory…"
        class="qt-input qt-input-sm"
        aria-label="Search the inventory by name"
      />
      <div qtBrowserFilters class="flex flex-wrap items-center gap-1" role="tablist" aria-label="Object kind">
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
      @if (equip()) {
        <label qtBrowserTogglesBefore class="flex items-center gap-2 qt-text-xs qt-text-secondary">
          <input
            type="checkbox"
            [checked]="equippedOnly()"
            (change)="equippedOnly.set($any($event.target).checked)"
            class="qt-checkbox"
          />
          Equipped only
        </label>
      }
      <button qtBrowserFooter type="button" class="qt-button-ghost qt-button-sm" [disabled]="!container()" (click)="fromArchetype()">
        + From archetype
      </button>
      <button qtBrowserFooter type="button" class="qt-button-ghost qt-button-sm" [disabled]="!container()" (click)="create('kit')">
        + New Kit
      </button>
      <button qtBrowserFooter type="button" class="qt-button-ghost qt-button-sm" [disabled]="!container()" (click)="create('archetype')">
        + New Archetype
      </button>
      <button qtBrowserFooter type="button" class="qt-button-primary qt-button-sm" [disabled]="!container()" (click)="create('item')">
        + New Item
      </button>
    </qt-folder-browser-shell>

    <ng-template #row let-item>
      <qt-inventory-item-row
        [item]="item"
        [manageable]="data().canManage(item)"
        [actions]="rowActions()"
        [equip]="rowEquipFor(item)"
        [selection]="selectionFor(item)"
        [folderCrumb]="state.searching() && item.folder ? crumb(item.folder) : null"
      />
    </ng-template>
  `,
})
export class InventoryBrowser {
  private readonly core = inject(CoreClient);
  private readonly realtime = inject(RealtimeService);

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

  protected readonly kinds = KINDS;
  protected readonly kindLabel = KIND_LABEL;
  protected readonly crumb = folderCrumb;
  /** v4 pin `:187-191`. */
  protected readonly text = {
    noContainer: 'Select an inventory to browse.',
    folderEmpty: 'Nothing on this shelf yet.',
    noMatch: 'Nothing matches this filter.',
  };

  protected readonly showShared = signal(true);
  protected readonly equippedOnly = signal(false);
  protected readonly kindFilter = signal<KindFilter>('items');
  protected readonly search = signal('');

  protected readonly isCharacterScope = computed(() => this.container()?.scope === 'character');
  private readonly carriedIds = computed(() => new Set((this.equip()?.carried.equipped ?? []).map((i) => i.id)));
  private readonly tierOf = computed(() => {
    const canManage = this.data().canManage;
    return (item: ResolvedInventoryItem): string => tierOfItem(item, canManage(item));
  });

  /** v4 `:91-107` — the filters, then the `base`-sensitivity sort by label. */
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

  /** v4 `:109-118`. */
  protected readonly state = createFolderBrowserState<ResolvedInventoryItem>({
    containerKey: () => {
      const c = this.container();
      return c ? encodeInventoryContainer(c) : '';
    },
    items: () => this.data().listItems(),
    filtered: this.filtered,
    tree: () => this.folders().folders(),
    tierOf: this.tierOf,
    characterView: this.isCharacterScope,
    showShared: this.showShared,
    searching: computed(() => this.search().trim().length > 0),
  });

  /** v4 `:122-139` — the folder writes, each refreshed afterwards (v4's onSettled). */
  protected readonly folderHandlers = createItemFolderHandlers<ResolvedInventoryItem>({
    state: this.state,
    ops: () => {
      const container = this.container();
      if (!container) return null;
      // v4 `600c2906f` (`use-item-folders.ts` `onSettled: refreshUnlessRealtime`):
      // the server publishes `inventory` from every location write (P4.D324),
      // so while the socket is up that publish does the re-read.
      const settled = <T>(p: Promise<T>): Promise<T> =>
        p.finally(() => void refreshUnlessRealtime(this.realtime, [() => this.actions().refresh()]));
      const ops: ItemFolderOps = {
        create: (path) => settled(createInventoryFolder(this.core, container, path)),
        rename: (path, name) => settled(renameInventoryFolder(this.core, container, path, name)),
        remove: (path) => settled(deleteInventoryFolder(this.core, container, path)),
      };
      return ops;
    },
    wording: {
      noun: 'object',
      rootPhrase: 'the top of the inventory',
      created: (name) => `The folder "${name}" is ready for the stock`,
      movedUp: (count) => `Folder removed; ${count} moved up`,
    },
    confirm: (message) => this.actions().requestConfirmation(message),
    describeError: describeInventoryRefusal,
  });

  /** v4 `:177-184` — the archived / shared pair (*Equipped only* is the before slot). */
  protected readonly toggles = computed<BrowserToggles>(() => ({
    archived: { checked: this.showArchived(), onChange: (next) => this.showArchivedChange.emit(next) },
    shared: this.isCharacterScope() ? { checked: this.showShared(), onChange: (next) => this.showShared.set(next) } : undefined,
  }));

  /** v4 `:204-207`. */
  protected readonly folderFiling: BrowserFolderFiling<ResolvedInventoryItem> = {
    open: (items, mode) => this.actions().openFolderPicker(items, mode),
    copyBlocked: (selected) =>
      selected.some((i) => !i.canCopy) ? 'Some of these objects cannot be copied' : undefined,
  };

  /** v4 `:152-162` — one stable object for every row. */
  protected readonly rowActions = computed<InventoryRowActions>(() => {
    const actions = this.actions();
    return {
      onEdit: (item) => actions.openEditor(item),
      onInstantiate: (item) => actions.openInstantiateArchetype(item),
      onDetach: (item) => void actions.detach(item),
      onDuplicate: (item) => void actions.duplicate(item),
      onMoveToFolder: (item) => actions.openFolderPicker([item], 'move'),
      onCopyToFolder: (item) => actions.openFolderPicker([item], 'copy'),
      onTransfer: (item, mode) => actions.openTransfer(item, mode),
      onToggleArchived: (item) => void actions.toggleArchived(item),
      onDelete: (item) => void actions.remove(item),
    };
  });

  /** v4 `rowEquip` (`:141-150`), memoised per row (a fresh literal each pass trips NG0100). */
  private readonly rowEquips = computed(() => {
    const equip = this.equip();
    if (!equip) return null;
    const carried = this.carriedIds();
    return new Map<string, InventoryRowEquip>(
      this.state.visible().map((item) => [
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

  /** The row's `selection` (memoised per row — NG0100); v4 passes `state.toggleSelected`. */
  private readonly selections = computed(() => {
    if (!this.state.selectMode()) return null;
    const ids = this.state.selectedIds();
    return new Map(
      this.state.visible().map((item) => [
        item.id,
        { selected: ids.has(item.id), onToggle: (i: ResolvedInventoryItem) => this.state.toggleSelected(i) },
      ]),
    );
  });

  protected selectionFor(
    item: ResolvedInventoryItem,
  ): { selected: boolean; onToggle: (item: ResolvedInventoryItem) => void } | undefined {
    return this.selections()?.get(item.id);
  }

  /** v4 `createFolder = inOwnTier ? currentFolder : ''`. */
  private createFolderPath(): string {
    return this.state.inOwnTier() ? this.state.currentFolder() : '';
  }

  protected create(mode: 'item' | 'kit' | 'archetype'): void {
    this.actions().openCreate(mode, this.createFolderPath());
  }

  protected fromArchetype(): void {
    const container = this.container();
    if (container) this.actions().openInstantiateInto(container, this.createFolderPath());
  }
}
