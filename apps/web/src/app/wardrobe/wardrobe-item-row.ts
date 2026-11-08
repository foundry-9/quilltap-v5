import {
  booleanAttribute,
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
  output,
  signal,
  viewChild,
  type TemplateRef,
} from '@angular/core';

import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { isComposite } from './equipped-slots';
import { WARDROBE_SLOT_META } from './slot-meta';
import { SlotBadge } from './slot-ui';
import { wardrobeOriginLabel } from './wardrobe-container';
import { formatPronounGuidance } from './pronoun-guidance';
import { formatWearLine, wearOf } from './wear-display';
import { WardrobeItemThumbnail } from './wardrobe-item-thumbnail';
import { ItemActionMenu, type MenuEntry, type MenuEntryLike, type MenuTrigger } from '../item-browser/item-action-menu';

/**
 * One line in the dialog's wardrobe list (v4
 * `components/wardrobe/wardrobe-item-row.tsx`). Shows the item title (allowed
 * to wrap to two lines, hover tooltip for the full title), slot-color chips,
 * and three controls:
 *
 *  - Primary equip button (`Wear` / `Try on`, label depends on which right-
 *    column tab is active).
 *  - `[+]` icon that adds the item to a slot. Single-slot items target their
 *    only slot directly; multi-slot items open the shared menu as a slot
 *    picker (each entry the slot's name and badge).
 *  - `⋮` kebab menu with secondary actions: Edit, Generate image, toggle the
 *    default-outfit flag, Duplicate, Archive, Move to folder…, Copy to
 *    folder…, Move to another wardrobe…, Copy to another wardrobe…, and
 *    Delete. Which of these appear
 *    is governed by the `canManage` predicate: items living in the container
 *    being browsed get the full set, items merged in from another shared tier
 *    keep only the two cross-wardrobe actions (a borrowed item is filed by
 *    whoever manages its own wardrobe).
 *
 * In the browser's select mode a manageable row shows a checkbox and its
 * other controls go quiet; a search hit shows the folder it lives in (v4
 * `4a38222e6` `:22-23`).
 *
 * Composite items keep a `▶/▼` expander so the user can peek at the
 * components without entering the editor (nested rows are read-only browse:
 * `inChat=false`, v4 `:397`).
 *
 * When the item has a current picture, a 40 px thumbnail sits at the left of
 * the title block; an item without one shows nothing there. With
 * `onOpenImage` the thumbnail opens the picture full screen (v4 `01a83539d`
 * `:24-26`; here the {@link canOpenImage} gate + the `openImage` output).
 *
 * **Recorded mechanism divergence — {@link canArchive}.** v4's
 * `onToggleArchived` is an OPTIONAL prop: a surface that must never archive
 * (the outfit composer, which does the same job the outfit-selection LLM does)
 * simply omits it and the entry does not render. An Angular `output` is always
 * present, so the same gate is an explicit boolean input, defaulting to FALSE —
 * absence, not presence, is what the two spellings share. {@link canMoveToFolder}
 * and {@link canCopyToFolder} are v4's optional `onMoveToFolder` /
 * `onCopyToFolder` in the same idiom.
 *
 * Both menus — the `⋮` and a multi-slot item's `+` slot picker — are the
 * shared `qt-item-action-menu` (v4 `b2b783b47` / `600c2906f`): fixed to the
 * viewport, keyboard-operable, closed by an outside press or Escape (the
 * Escape swallowed so the enclosing dialog stays open).
 */
@Component({
  selector: 'qt-wardrobe-item-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ItemActionMenu, WardrobeItemThumbnail, SlotBadge],
  // The host needs a BOX: it is a direct child of a `space-y-*` stack, and a
  // vertical margin never applies to a non-replaced inline box — see
  // `_surfaces.css`'s rule for this class (P4.75, measured).
  host: {
    class: 'qt-wardrobe-item-row',
  },
  template: `
    <div
      class="qt-card-interactive py-2 px-3"
      [style.margin-left]="depth() > 0 ? depth() * 12 + 'px' : null"
    >
      <div class="flex items-start gap-2">
        <!-- Select mode (v4 4a38222e6 :199-211): a manageable row's checkbox,
             a borrowed row's spacer. -->
        @if (selection(); as sel) {
          @if (manageable()) {
            <input
              type="checkbox"
              class="qt-checkbox mt-1"
              [checked]="sel.selected"
              (change)="sel.onToggle()"
              [attr.aria-label]="'Select ' + item().title"
            />
          } @else {
            <span class="inline-block w-4" aria-hidden="true"></span>
          }
        }
        @if (isComposite()) {
          <button
            type="button"
            class="qt-text-secondary hover:text-foreground mt-0.5"
            [attr.aria-label]="expanded() ? 'Collapse components' : 'Expand components'"
            (click)="expanded.set(!expanded())"
          >
            {{ expanded() ? '▼' : '▶' }}
          </button>
        } @else {
          <span class="inline-block w-3" aria-hidden="true"></span>
        }

        <qt-wardrobe-item-thumbnail
          [fileId]="item().imageFileId"
          [size]="40"
          [openable]="canOpenImage()"
          [openLabel]="'View the picture of ' + item().title"
          (open)="openImage.emit(item())"
        />

        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2 flex-wrap">
            <span
              class="text-sm text-foreground"
              style="display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; word-break: break-word; max-width: 100%; min-width: 0"
              [title]="item().title"
            >
              {{ item().title }}
            </span>
            @if (isComposite()) {
              <span class="qt-text-xs qt-text-secondary">· bundle</span>
            }
            @if (item().isDefault) {
              <span class="qt-text-xs qt-text-secondary">· default</span>
            }
            @if (item().archivedAt) {
              <span class="qt-badge qt-badge-secondary">archived</span>
            }
            @for (t of item().types; track t) {
              <qt-slot-badge [slot]="t" />
            }
            <!-- The origin chip (v4 cc80dc89d :250-256): only a BORROWED
                 row whose origin resolves to a label — a pre-round server's
                 rows carry no origin and so show nothing. -->
            @if (!manageable() && originLabel()) {
              <span class="qt-badge qt-badge-wardrobe-shared" [title]="'Borrowed from ' + originLabel()">{{
                originLabel()
              }}</span>
            }
          </div>
          <!-- A search hit's folder (v4 4a38222e6 :269-273). -->
          @if (folderCrumb()) {
            <div class="qt-text-xs qt-text-secondary mt-0.5" data-testid="wardrobe-folder-crumb">
              {{ folderCrumb() }}
            </div>
          }
          <!-- Wear ledger tally (v4 3ee3b1342 :257-263) — the count is what
               gets compared across rows; the full breakdown is the editor's
               job. Always rendered: an absent annotation reads Never worn. -->
          <div class="qt-text-xs qt-text-secondary mt-0.5" data-testid="wardrobe-wear-line">
            {{ wearLine() }}
          </div>
          @if (item().appropriateness) {
            <div class="qt-text-xs qt-text-secondary truncate mt-0.5">
              {{ item().appropriateness }}
            </div>
          }
          <!-- Pronoun guidance (v4 eb90d9ed8, wardrobe-item-row.tsx:289-293). -->
          @if ((item().pronounGuidance?.length ?? 0) > 0) {
            <div
              class="qt-text-xs qt-text-secondary truncate mt-0.5"
              data-testid="wardrobe-pronoun-guidance"
            >
              For {{ pronounGuidanceText() }}
            </div>
          }
        </div>

        <div class="flex items-center gap-1 flex-shrink-0">
          <!-- Primary equip button — label depends on the active right-column
               tab (Wear in Live outfit, Try on in Outfit Builder). -->
          @if (inChat()) {
            <button
              type="button"
              (click)="equip.emit(item())"
              [disabled]="selectMode()"
              class="qt-button-ghost qt-button-sm"
              title="Puts it on across every slot it covers — layers or replaces per the item's setting"
            >
              {{ equipLabel() }}
            </button>

            <!-- Single-icon add button. For single-typed items it adds
                 directly; for multi-typed it opens a slot picker — the shared
                 menu, fixed to the viewport so the dialog's scrolling list
                 cannot clip it (v4 pin :298-314). -->
            @if (item().types.length === 1) {
              <button
                type="button"
                (click)="addToSlot.emit({ item: item(), slot: item().types[0] })"
                [disabled]="selectMode()"
                class="qt-button-ghost qt-button-sm"
                [title]="addTooltip()"
                [attr.aria-label]="addTooltip()"
              >
                +
              </button>
            } @else {
              <qt-item-action-menu
                [label]="addTooltip()"
                [trigger]="addTrigger()"
                [disabled]="selectMode()"
                [room]="240"
                minWidthClass="min-w-[10rem]"
                [entries]="slotEntries()"
              />
            }
          }

          <!-- Kebab menu — item actions (v4 pin :319-…). -->
          <qt-item-action-menu [disabled]="selectMode()" [entries]="kebabEntries()" />
        </div>
      </div>

      <ng-template #slotLabel let-entry>
        <span class="flex items-center justify-between gap-2">
          <span>{{ entry.label }}</span>
          <qt-slot-badge [slot]="slotOf(entry)" />
        </span>
      </ng-template>

      <!-- Nested components (read-only here; click Edit to change). A nested
           row cannot be filed, selected or archived from its parent: none of
           canMoveToFolder / canCopyToFolder / selection / folderCrumb /
           canArchive is forwarded
           (v4 4a38222e6 :511-529). -->
      @if (isComposite() && expanded()) {
        <div class="mt-2 border-l-2 qt-border-default pl-2 space-y-1">
          @if (components().length === 0) {
            <div class="qt-text-xs qt-text-secondary px-2 py-1">
              Components missing from this wardrobe.
            </div>
          } @else {
            @for (c of components(); track c.id) {
              <qt-wardrobe-item-row
                [item]="c"
                [allItems]="allItems()"
                [inChat]="false"
                [canManage]="canManage()"
                [canGenerateImage]="canGenerateImage()"
                [generatingImageIds]="generatingImageIds()"
                [canOpenImage]="canOpenImage()"
                [depth]="depth() + 1"
                (toggleDefault)="toggleDefault.emit($event)"
                (edit)="edit.emit($event)"
                (duplicate)="duplicate.emit($event)"
                (move)="move.emit($event)"
                (copyItem)="copyItem.emit($event)"
                (delete)="delete.emit($event)"
                (generateImage)="generateImage.emit($event)"
                (openImage)="openImage.emit($event)"
              />
            }
          }
        </div>
      }
    </div>
  `,
})
export class WardrobeItemRow {
  readonly item = input.required<WardrobeItemDto>();

  /** v4 `formatPronounGuidance(item.pronounGuidance)` — the `For …` note's labels. */
  protected readonly pronounGuidanceText = computed(() =>
    formatPronounGuidance(this.item().pronounGuidance),
  );
  /** All items in the cache (this character + shared archetypes) — used to
   *  render composite components inline. */
  readonly allItems = input.required<WardrobeItemDto[]>();
  /** When set, equip controls are visible. */
  readonly inChat = input.required<boolean>();
  /**
   * Whether an item can be managed (edited / starred / duplicated / deleted)
   * from the current view — true when the item lives in the container being
   * browsed, false when it was merged in from a shared tier elsewhere. Items
   * failing this check keep only Move and Copy, and — when their collection
   * read named where they came from — carry a chip naming the wardrobe they
   * were borrowed from (`Project · Thornfield`; v4 `cc80dc89d`).
   * Defaults to the character-view rule: manageable iff character-owned
   * (v4 `wardrobe-item-row.tsx:36-40`, `:83-85`).
   */
  readonly canManage = input<((item: WardrobeItemDto) => boolean) | undefined>(undefined);
  /** Label for the equip-replace button (v4 default "Wear"). */
  readonly equipLabel = input('Wear');
  /** Whether `[+]` is framed as "layer onto" (Live) or "add to" (Builder) —
   *  tooltip only (v4 `:33-37`). */
  readonly addAction = input<'layer' | 'add'>('layer');
  readonly isUpdatingDefault = input(false);
  /**
   * Whether this surface may archive at all — v4's optional `onToggleArchived`
   * prop, expressed as a gate (see the class header). Default false so a new
   * host offers nothing until it says otherwise; the entry additionally rides
   * inside `canManage`, alongside Edit and Duplicate.
   */
  readonly canArchive = input(false, { transform: booleanAttribute });
  /**
   * Whether this host offers "Generate image" at all — v4's optional
   * `onGenerateImage` prop, expressed as a gate (the {@link canArchive}
   * precedent: an Angular output always exists). Default false; the entry
   * additionally rides inside `canManage`, directly after Edit (v4
   * `7c8572869` `:361-373`).
   */
  readonly canGenerateImage = input(false, { transform: booleanAttribute });
  /**
   * Item ids whose picture is being drawn right now — a SET, since an item
   * can appear twice (as itself and as a component of an outfit row). Passed
   * down to nested component rows (v4 `:217-219`).
   */
  readonly generatingImageIds = input<ReadonlySet<string>>(new Set());
  /**
   * Whether the thumbnail opens the item's current picture full screen — v4's
   * optional `onOpenImage` prop (`01a83539d`, `:89-90`: "Open the item's
   * current picture full screen. Passed down to nested rows."), expressed as a
   * gate (the {@link canGenerateImage} precedent). Default false.
   */
  readonly canOpenImage = input(false, { transform: booleanAttribute });
  /**
   * Whether this host files items into folders at all — v4's optional
   * `onMoveToFolder` prop (`4a38222e6` `:77-78`: "File the item in another
   * folder of its own wardrobe (manageable rows only)"), as a gate (the
   * {@link canArchive} precedent). Default false; the entry additionally
   * rides on `canManage`, after Archive and before the cross-wardrobe pair.
   */
  readonly canMoveToFolder = input(false, { transform: booleanAttribute });
  /**
   * v4's optional `onCopyToFolder` (`bbbcd2388` `:80`: file a COPY in a folder
   * of its own wardrobe, manageable rows only) as a gate — its entry sits
   * immediately after **Move to folder…**.
   */
  readonly canCopyToFolder = input(false, { transform: booleanAttribute });
  /**
   * Select mode (v4 `4a38222e6` `:79-83`): a checkbox on a manageable row
   * (never on a borrowed one), and the wear buttons and `⋮` disabled while it
   * is on. Undefined = not selecting.
   */
  readonly selection = input<{ selected: boolean; onToggle: () => void } | undefined>(undefined);
  /** A search hit's folder, shown as a muted crumb (`Formal › Evening`; v4
   *  `:84-85`). */
  readonly folderCrumb = input<string | null>(null);
  /** Nesting depth for composite components — indentation. */
  readonly depth = input(0);

  readonly toggleDefault = output<WardrobeItemDto>();
  readonly edit = output<WardrobeItemDto>();
  readonly duplicate = output<WardrobeItemDto>();
  readonly move = output<WardrobeItemDto>();
  readonly copyItem = output<WardrobeItemDto>();
  readonly delete = output<WardrobeItemDto>();
  /** Archive an active garment, or restore an archived one (v4 `d25dacc1`). */
  readonly toggleArchived = output<WardrobeItemDto>();
  /** Draw a picture of this item with the designated wardrobe profile. */
  readonly generateImage = output<WardrobeItemDto>();
  /** Open this item's (or a nested component's) current picture full screen. */
  readonly openImage = output<WardrobeItemDto>();
  /** File the item in another folder of its own wardrobe (v4 `onMoveToFolder`). */
  readonly moveToFolder = output<WardrobeItemDto>();
  /** File a copy of the item in a folder of its own wardrobe (v4 `onCopyToFolder`). */
  readonly copyToFolder = output<WardrobeItemDto>();
  readonly equip = output<WardrobeItemDto>();
  readonly addToSlot = output<{ item: WardrobeItemDto; slot: WardrobeSlotType }>();

  protected readonly slotMeta = WARDROBE_SLOT_META;

  protected readonly expanded = signal(false);

  protected readonly isGeneratingImage = computed(() =>
    this.generatingImageIds().has(this.item().id),
  );

  protected readonly isComposite = computed(() => isComposite(this.item()));
  /** v4 `:147` — `selection !== undefined`. */
  protected readonly selectMode = computed(() => this.selection() !== undefined);

  private readonly slotLabel = viewChild.required<TemplateRef<unknown>>('slotLabel');
  /**
   * v4 `:83-85` — without an explicit predicate, fall back to the
   * character-view rule: personal items are manageable, shared-tier items are
   * Move/Copy only.
   */
  protected readonly manageable = computed(() => {
    const predicate = this.canManage();
    return predicate ? predicate(this.item()) : Boolean(this.item().characterId);
  });

  /** v4 `:261-263` — `formatWearLine(wearOf(item))`. */
  protected readonly wearLine = computed(() => formatWearLine(wearOf(this.item())));

  /** v4 `:193` — `wardrobeOriginLabel(item.origin)`. */
  protected readonly originLabel = computed(() => wardrobeOriginLabel(this.item().origin));

  protected readonly components = computed(() => {
    if (!this.isComposite()) return [];
    const byId = new Map(this.allItems().map((i) => [i.id, i]));
    return (this.item().componentItemIds ?? [])
      .map((id) => byId.get(id))
      .filter((c): c is WardrobeItemDto => Boolean(c));
  });

  /** v4 `:152-157`. */
  protected readonly addTooltip = computed(() => {
    const types = this.item().types;
    if (types.length === 1) {
      return `${this.addAction() === 'layer' ? 'Layer onto' : 'Add to'} ${WARDROBE_SLOT_META[types[0]].label.toLowerCase()}`;
    }
    return this.addAction() === 'layer' ? 'Layer onto a slot' : 'Add to a slot';
  });

  /** v4 `trigger={{ content: '+', ariaLabel: addTooltip }}`. */
  protected readonly addTrigger = computed<MenuTrigger>(() => ({ content: '+', ariaLabel: this.addTooltip() }));

  /** v4 pin `:304-312` — one entry per slot, labelled by the slot name + its badge. */
  protected readonly slotEntries = computed<readonly MenuEntryLike[]>(() => {
    const item = this.item();
    const template = this.slotLabel();
    return item.types.map((slot) => ({
      key: `slot-${slot}`,
      label: WARDROBE_SLOT_META[slot].label,
      template,
      onSelect: () => this.addToSlot.emit({ item, slot }),
    }));
  });

  protected slotOf(entry: MenuEntry): WardrobeSlotType {
    return entry.key.slice('slot-'.length) as WardrobeSlotType;
  }

  /** v4 pin `:321-347` — the row's actions in v4's order, `manageable && …` holes dropped. */
  protected readonly kebabEntries = computed<readonly MenuEntryLike[]>(() => {
    const item = this.item();
    const manageable = this.manageable();
    const generating = this.isGeneratingImage();
    return [
      manageable && { key: 'edit', label: 'Edit', onSelect: () => this.edit.emit(item) },
      manageable &&
        this.canGenerateImage() && {
          key: 'generate-image',
          label: generating ? 'Generating image…' : 'Generate image',
          disabled: generating,
          onSelect: () => this.generateImage.emit(item),
        },
      manageable && {
        key: 'default',
        label: item.isDefault ? '☆ Unmark as default' : '★ Mark as default outfit item',
        disabled: this.isUpdatingDefault(),
        onSelect: () => this.toggleDefault.emit(item),
      },
      manageable && { key: 'duplicate', label: 'Duplicate', onSelect: () => this.duplicate.emit(item) },
      manageable &&
        this.canArchive() && {
          key: 'archive',
          label: item.archivedAt ? 'Restore from archive' : 'Archive',
          onSelect: () => this.toggleArchived.emit(item),
        },
      manageable &&
        this.canMoveToFolder() && { key: 'move-to-folder', label: 'Move to folder…', onSelect: () => this.moveToFolder.emit(item) },
      manageable &&
        this.canCopyToFolder() && { key: 'copy-to-folder', label: 'Copy to folder…', onSelect: () => this.copyToFolder.emit(item) },
      { key: 'transfer-move', label: 'Move to another wardrobe…', onSelect: () => this.move.emit(item) },
      { key: 'transfer-copy', label: 'Copy to another wardrobe…', onSelect: () => this.copyItem.emit(item) },
      manageable && { key: 'delete', label: 'Delete', destructive: true, onSelect: () => this.delete.emit(item) },
    ];
  });
}
