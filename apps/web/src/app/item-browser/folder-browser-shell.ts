import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  Directive,
  computed,
  contentChild,
  input,
  type TemplateRef,
} from '@angular/core';

import type { FolderBrowserState, ItemFolderHandlers } from './folder-browser-state';
import { OWN_TIER, type BrowserViewItem, type FolderRowView } from './folder-view';
import type { MenuEntryLike } from './item-action-menu';
import { ItemFolderBar, NewFolderInput } from './item-folder-bar';
import { ItemFolderRow } from './item-folder-row';
import { ItemSelectBar } from './item-select-bar';
import { ItemTierRow } from './item-tier-row';

type Item = BrowserViewItem & { id: string };

/** A labelled checkbox in the toggles row (v4 `BrowserToggle`). */
export interface BrowserToggle {
  checked: boolean;
  onChange: (next: boolean) => void;
}

/** The toggles row's shared pair (v4 `BrowserToggles` minus its `before` / `after` nodes — those are slots here). */
export interface BrowserToggles {
  /** *Show archived* — re-reads with archived rows rather than filtering. */
  archived?: BrowserToggle;
  /**
   * *Show shared* — a character's merged view only. Turning it off from
   * inside a borrowed tier goes back to the character's own root.
   */
  shared?: BrowserToggle;
}

/** The select bar's *Move to folder…* / *Copy to folder…* pair (v4 `BrowserFolderFiling`). */
export interface BrowserFolderFiling<T> {
  open: (items: T[], mode: 'move' | 'copy') => void;
  /** Why the selection cannot be copied, or undefined when it can. */
  copyBlocked?: (items: T[]) => string | undefined;
}

/** Marks content projected BEFORE the shared toggles (v4 `toggles.before` — the Inventory's *Equipped only*). */
@Directive({ selector: '[qtBrowserTogglesBefore]' })
export class BrowserTogglesBefore {}

/** Marks content projected AFTER the shared toggles (v4 `toggles.after` — the wardrobe's *Never worn*). */
@Directive({ selector: '[qtBrowserTogglesAfter]' })
export class BrowserTogglesAfter {}

/** One folder row's memoised bindings (a fresh closure per check would trip NG0100). */
interface FolderRowBinding {
  row: FolderRowView;
  rename: (name: string) => Promise<boolean>;
  extra: readonly MenuEntryLike[];
}

/**
 * The layout every folder-of-items browser shares — a port of v4
 * `components/item-browser/FolderBrowserShell.tsx` at the pin `624d0d25c`
 * (NEW at `b2b783b47`; `c34986421`'s `manageFolders`; `600c2906f`'s toggles
 * and folder filing).
 *
 * v4's module doc, carried: the browser's own filter strip, the folder bar
 * (breadcrumb, New folder, Select) and the inline new-folder input, then the
 * list — folder rows, a character's borrowed tier rows, the empty / no-match
 * states, the item rows — the select bar with the browser's filing buttons,
 * and the sticky footer of create buttons. The state is
 * `createFolderBrowserState`'s and the folder writes
 * `createItemFolderHandlers`'; this component only lays them out.
 *
 * v5 shape (L-2): v4's `renderItem` is the required `itemTemplate`; v4's
 * `ReactNode` props are named projection slots — `[qtBrowserFilters]`,
 * `[qtBrowserTogglesBefore]` / `[qtBrowserTogglesAfter]` (directives, so the
 * shell can tell when one is used: the toggles row renders when `toggles` is
 * set OR either toggle slot holds content — v4 renders it whenever `toggles`
 * is passed, and its `before` / `after` ride inside `toggles`),
 * `[qtBrowserSelectActions]`, `[qtBrowserFooter]`, and the default slot
 * (v4's `children` — dialogs the browser mounts).
 */
@Component({
  selector: 'qt-folder-browser-shell',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ItemFolderBar, ItemFolderRow, ItemSelectBar, ItemTierRow, NewFolderInput, NgTemplateOutlet],
  template: `
    <section class="flex flex-col min-h-0 relative">
      <div class="flex flex-col gap-2 mb-2">
        <ng-content select="[qtBrowserFilters]" />
        @if (showToggles()) {
          <div class="flex flex-wrap items-center gap-x-4 gap-y-1">
            <ng-content select="[qtBrowserTogglesBefore]" />
            @if (toggles()?.archived; as archived) {
              <label class="flex items-center gap-2 qt-text-xs qt-text-secondary">
                <input
                  type="checkbox"
                  [checked]="archived.checked"
                  (change)="archived.onChange($any($event.target).checked)"
                  class="qt-checkbox"
                />
                Show archived
              </label>
            }
            @if (toggles()?.shared; as shared) {
              <label class="flex items-center gap-2 qt-text-xs qt-text-secondary">
                <input
                  type="checkbox"
                  [checked]="shared.checked"
                  (change)="onSharedChange(shared, $any($event.target).checked)"
                  class="qt-checkbox"
                />
                Show shared
              </label>
            }
            <ng-content select="[qtBrowserTogglesAfter]" />
          </div>
        }
        @if (hasContainer()) {
          <qt-item-folder-bar
            [rootLabel]="rootLabel()"
            [navLabel]="rootLabel() + ' folders'"
            [searching]="state().searching()"
            [tierLabel]="state().inOwnTier() ? null : state().currentTierLabel()"
            [currentFolder]="state().currentFolder()"
            (navigateRoot)="state().navigate(ownTier, '')"
            (navigateTier)="state().navigate(state().currentTier(), '')"
            (navigateFolder)="state().navigate(state().currentTier(), $event)"
            [managed]="managed()"
            [newFolderOpen]="state().newFolder.open()"
            (newFolder)="state().newFolder.setName('')"
            [selectMode]="state().selectMode()"
            (toggleSelect)="state().setSelectMode(!state().selectMode())"
            [testId]="testIdPrefix() + '-folder-bar'"
          />
        }
        @if (state().newFolder.open() && managed() && !state().searching()) {
          <qt-new-folder-input
            [name]="state().newFolder.name()"
            (nameChange)="state().newFolder.setName($event)"
            (submitted)="folderHandlers().submitNewFolder()"
            (cancelled)="state().newFolder.close()"
          />
        }
      </div>

      <div [attr.class]="listClass()">
        @if (!hasContainer()) {
          <div class="text-sm qt-text-secondary px-3 py-4">{{ text().noContainer }}</div>
        } @else if (loading()) {
          <div class="text-sm qt-text-secondary px-3 py-4">Loading…</div>
        } @else {
          @for (b of folderRowBindings(); track b.row.path) {
            <qt-item-folder-row
              [row]="b.row"
              [manageable]="managed()"
              (opened)="state().navigate(state().currentTier(), b.row.path)"
              [rename]="b.rename"
              (deleted)="folderHandlers().deleteFolder(b.row.path)"
              [extraEntries]="b.extra"
              [testId]="testIdPrefix() + '-folder-row'"
              [countNoun]="countNoun()"
            />
          }
          @for (row of state().sharedTierRows(); track row.key) {
            <qt-item-tier-row
              [label]="row.label"
              [scope]="row.scope"
              [itemCount]="row.itemCount"
              (opened)="state().navigate(row.key, '')"
              [testId]="testIdPrefix() + '-tier-row'"
              [countNoun]="countNoun()"
            />
          }
          @if (state().folderIsEmpty()) {
            <div class="text-sm qt-text-secondary px-3 py-4">{{ text().folderEmpty }}</div>
          } @else if (
            state().visible().length === 0 && state().folderRows().length === 0 && state().sharedTierRows().length === 0
          ) {
            <div class="text-sm qt-text-secondary px-3 py-4">{{ text().noMatch }}</div>
          } @else {
            @for (item of state().visible(); track item.id) {
              <ng-container [ngTemplateOutlet]="itemTemplate()" [ngTemplateOutletContext]="{ $implicit: item }" />
            }
          }
          @if (state().selectMode()) {
            <qt-item-select-bar
              [count]="state().selectedIds().size"
              [sticky]="scrollList()"
              [testId]="testIdPrefix() + '-select-bar'"
              (cancelled)="state().setSelectMode(false)"
            >
              @if (folderFiling(); as filing) {
                <button
                  type="button"
                  class="qt-button-secondary qt-button-sm"
                  [disabled]="state().selectedItems().length === 0"
                  (click)="filing.open(state().selectedItems(), 'move')"
                >
                  Move to folder…
                </button>
                <button
                  type="button"
                  class="qt-button-secondary qt-button-sm"
                  [disabled]="state().selectedItems().length === 0 || copyBlocked() !== undefined"
                  [attr.title]="copyBlocked() ?? null"
                  (click)="filing.open(state().selectedItems(), 'copy')"
                >
                  Copy to folder…
                </button>
              }
              <ng-content select="[qtBrowserSelectActions]" />
            </qt-item-select-bar>
          }
        }
      </div>

      <div
        class="sticky bottom-0 -mx-1 px-1 pt-2 pb-1 qt-bg-default border-t qt-border-default flex flex-wrap items-center gap-2 justify-end"
      >
        <ng-content select="[qtBrowserFooter]" />
      </div>

      <ng-content />
    </section>
  `,
})
export class FolderBrowserShell<T extends Item> {
  readonly state = input.required<FolderBrowserState<T>>();
  /** False while no container is chosen. */
  readonly hasContainer = input(true);
  readonly loading = input(false);
  /** Prefix of every `data-testid`, e.g. `wardrobe` → `wardrobe-folder-row`. */
  readonly testIdPrefix = input.required<string>();
  /** The root crumb, e.g. `Wardrobe`. */
  readonly rootLabel = input.required<string>();
  /** The noun folder and tier rows count, e.g. `item`. */
  readonly countNoun = input('item');
  readonly text = input.required<{ noContainer: string; folderEmpty: string; noMatch: string }>();
  /** The toggles row's shared pair. */
  readonly toggles = input<BrowserToggles | undefined>(undefined);
  /** One item row (v4 `renderItem`). */
  readonly itemTemplate = input.required<TemplateRef<{ $implicit: T }>>();
  /** The select bar's folder-filing pair, ahead of the `[qtBrowserSelectActions]` slot. */
  readonly folderFiling = input<BrowserFolderFiling<T> | undefined>(undefined);
  /** Folder handlers from `createItemFolderHandlers`. */
  readonly folderHandlers = input.required<ItemFolderHandlers>();
  /** More ⋮ entries on a folder row (Move to…, Copy to…). */
  readonly folderEntries = input<((row: FolderRowView) => readonly MenuEntryLike[]) | undefined>(undefined);
  /**
   * Whether folders here can be made, renamed and deleted; defaults to the
   * state's `managed` (a read-only document store turns it off).
   */
  readonly manageFolders = input<boolean | undefined>(undefined);
  /** Cap the list's height (a dialog) instead of letting it grow (a card). */
  readonly scrollList = input(true);

  protected readonly ownTier = OWN_TIER;
  private readonly togglesBefore = contentChild(BrowserTogglesBefore);
  private readonly togglesAfter = contentChild(BrowserTogglesAfter);

  protected readonly showToggles = computed(
    () => this.toggles() !== undefined || !!this.togglesBefore() || !!this.togglesAfter(),
  );
  protected readonly managed = computed(() => this.state().managed() && (this.manageFolders() ?? true));
  protected readonly copyBlocked = computed(() => this.folderFiling()?.copyBlocked?.(this.state().selectedItems()));
  protected readonly listClass = computed(
    () => `flex-1 space-y-1 pb-12 ${this.scrollList() ? 'overflow-y-auto max-h-[55vh]' : ''}`,
  );
  protected readonly folderRowBindings = computed<FolderRowBinding[]>(() => {
    const handlers = this.folderHandlers();
    const entries = this.folderEntries();
    return this.state()
      .folderRows()
      .map((row) => ({
        row,
        rename: (name: string) => handlers.renameFolder(row.path, name),
        extra: entries ? entries(row) : [],
      }));
  });

  /** v4: unticking *Show shared* inside a borrowed tier navigates to the own root. */
  protected onSharedChange(toggle: BrowserToggle, checked: boolean): void {
    const inOwnTier = this.state().inOwnTier();
    toggle.onChange(checked);
    if (!checked && !inOwnTier) this.state().navigate(OWN_TIER, '');
  }
}
