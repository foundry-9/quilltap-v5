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

import { CoreClient } from '../core/core-client';
import {
  INVENTORY_SVG_MAX_BYTES,
  type InventoryInheritableField,
  type InventoryItemBody,
  type ResolvedInventoryItem,
} from '../core/core-contract';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import { GENERAL_INVENTORY_CONTAINER, type InventoryContainer } from './inventory-container';
import { InventoryPicture, inventorySvgDataUri } from './inventory-glyph';
import type { InventoryItemActions } from './inventory-item-actions';
import { InventoryPicturesSection } from './inventory-pictures-section';
import { describeInventoryRefusal } from './inventory-refusal';
import { clearInventoryOverrides, createInventoryItem, listInventory, updateInventoryItem } from './inventory.api';

/** v4 `Draft` (`:62-76`). */
export interface InventoryDraft {
  name: string;
  title: string;
  description: string;
  imageDescription: string;
  equippedByDefault: boolean;
  maxEquipped: number;
  canCopy: boolean;
  isArchetype: boolean;
  emoji: string;
  svg: string;
  folder: string;
  metadataText: string;
  itemIds: string[];
}

/** v4 `DRAFT_FIELD` (`:78-89`) — the draft keys an instance may inherit. */
export const DRAFT_FIELD: Partial<Record<keyof InventoryDraft, InventoryInheritableField>> = {
  name: 'name',
  title: 'title',
  description: 'description',
  imageDescription: 'imageDescription',
  equippedByDefault: 'equippedByDefault',
  maxEquipped: 'maxEquipped',
  canCopy: 'canCopy',
  emoji: 'emoji',
  svg: 'svg',
  metadataText: 'metadata',
};

/** v4 `draftOf` (`:91-107`). */
export function draftOf(
  item: ResolvedInventoryItem | null,
  mode: 'item' | 'kit' | 'archetype',
  folder: string,
): InventoryDraft {
  return {
    name: item?.name ?? '',
    title: item?.title ?? '',
    description: item?.description ?? '',
    imageDescription: item?.imageDescription ?? '',
    equippedByDefault: item?.equippedByDefault ?? false,
    maxEquipped: item?.maxEquipped ?? 1,
    canCopy: item?.canCopy ?? false,
    isArchetype: item?.isArchetype ?? mode === 'archetype',
    emoji: item?.emoji ?? '',
    svg: item?.svg ?? '',
    folder: item?.folder ?? folder,
    metadataText: item && Object.keys(item.metadata ?? {}).length > 0 ? JSON.stringify(item.metadata, null, 2) : '',
    itemIds: item?.itemIds ?? [],
  };
}

const orNull = (value: string): string | null => (value.trim() ? value.trim() : null);

/** v4 `:333`'s hint for "How it should be drawn" — `d88c1ce39`'s sentence. */
export const IMAGE_DESCRIPTION_HINT =
  "Rendering hints joined to the character's appearance while carried, and the subject of the object's own picture. Leave empty and the object is invisible in avatars, story backgrounds and every other picture of the character (its own picture falls back to the description above).";

/** The UTF-8 size of an SVG glyph, as v4 counts it (`TextEncoder`). */
export function svgByteLength(svg: string): number {
  return new TextEncoder().encode(svg).length;
}

/**
 * The Inventory object editor — create or edit one object, kit or archetype.
 * A port of v4 `components/inventory/inventory-item-editor.tsx` (`322a5488e`,
 * `102a23648`'s placeholders, `d88c1ce39`'s drawing hint).
 *
 * Every field: the name and title, the two descriptions, *Equipped by
 * default*, the per-chat cap, *Can be copied*, *Archetype* (not for an
 * instance), the glyph (emoji, or an SVG with a live preview and a size
 * counter), the folder and free-form metadata. A kit edits its members with a
 * picker over what the tier can reach — pictures shown, archetypes allowed,
 * kits and archived objects excluded, a project or group kit also offered
 * Quilltap General's. An existing object has a Pictures section.
 *
 * **An instance inherits.** Each inheritable field shows the archetype's value
 * greyed with an *Inherit* toggle; unticking makes the archetype's value the
 * instance's own override to edit, editing a field un-inherits it, and Save
 * sends `{ folder, …touched non-inherited fields }` then clears the overrides
 * the operator re-ticked (`inventoryClearOverride`).
 *
 * **Recorded mechanism divergences.** v4's `BaseModal` portal + Escape are
 * local here (`qt-modal` in place at `z-[70]`, a document Escape listener gated
 * by saving and by the picture viewer — the wardrobe picker's idiom). The
 * General read for a project / group kit is a one-shot `inventoryList`, where
 * v4 keeps a TanStack read.
 */
@Component({
  selector: 'qt-inventory-item-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, NgTemplateOutlet, InventoryPicture, InventoryPicturesSection],
  host: { '(document:keydown.escape)': 'onEscape()' },
  template: `
    <qt-modal
      [title]="title()"
      maxWidth="2xl"
      overlayClass="z-[70]"
      [closeOnBackdrop]="!saving()"
      (close)="closed.emit()"
    >
      <div class="space-y-3">
        <p class="qt-text-xs qt-text-secondary">
          {{ item() ? 'Kept in' : 'Will be kept in' }} {{ containerLabel() }}
          @if (isInstance() && parent(); as p) {
            · an instance of <strong>{{ p.title || p.name }}</strong>
          }
          @if (isInstance() && !parent()) {
            · an instance whose archetype is out of view
          }
        </p>
        <div class="grid md:grid-cols-2 gap-3">
          <ng-container *ngTemplateOutlet="textField; context: { key: 'name', label: 'Name', placeholder: isKit() ? 'burglar\\'s kit' : 'sword' }" />
          <ng-container *ngTemplateOutlet="textField; context: { key: 'title', label: 'Title', placeholder: isKit() ? 'The Night Shift' : 'Excalibur' }" />
        </div>
        <ng-container
          *ngTemplateOutlet="
            areaField;
            context: {
              key: 'description',
              label: 'What the characters know about it',
              hint: 'Told to the model while the object is carried. Leave empty and the object is silent to the model.',
              rows: 3,
            }
          "
        />
        <ng-container
          *ngTemplateOutlet="
            areaField;
            context: { key: 'imageDescription', label: 'How it should be drawn', hint: imageHint, rows: 2 }
          "
        />
        @if (isKit()) {
          <fieldset class="space-y-1">
            <legend class="text-sm text-foreground">Members</legend>
            <p class="qt-text-xs qt-text-secondary">
              Equipping the kit takes up every member; an archetype member is made into an instance for the character
              who equips it.
            </p>
            <ul class="max-h-56 overflow-y-auto space-y-1" aria-label="Kit members">
              @for (choice of memberChoices(); track choice.id) {
                <li>
                  <label class="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      class="qt-checkbox"
                      [checked]="draft().itemIds.includes(choice.id)"
                      (change)="toggleMember(choice.id)"
                    />
                    <qt-inventory-picture [item]="choice" [size]="32" />
                    <span class="flex-1 min-w-0 truncate">{{ choice.title || choice.name }}</span>
                    @if (choice.isArchetype) {
                      <span class="qt-badge qt-badge-primary">Archetype</span>
                    }
                  </label>
                </li>
              }
              @if (memberChoices().length === 0) {
                <li class="qt-text-xs qt-text-secondary">Nothing here to put in a kit yet.</li>
              }
            </ul>
          </fieldset>
        } @else {
          <div class="space-y-2">
            <ng-container
              *ngTemplateOutlet="
                checkField;
                context: {
                  key: 'equippedByDefault',
                  label: 'Equipped by default',
                  hint: 'Every character who can reach it arrives in a new chat carrying it.',
                }
              "
            />
            <ng-container
              *ngTemplateOutlet="
                checkField;
                context: {
                  key: 'canCopy',
                  label: 'Can be copied',
                  hint: 'Off refuses Duplicate and copy-transfers; moving and instantiating stay allowed.',
                }
              "
            />
            @if (!isInstance()) {
              <ng-container
                *ngTemplateOutlet="
                  checkField;
                  context: {
                    key: 'isArchetype',
                    label: 'Archetype',
                    hint: 'A class of object, never carried itself; characters get instances of it.',
                  }
                "
              />
            }
            <div class="flex items-center gap-2">
              <label class="text-sm text-foreground" for="inventory-maxEquipped">Carried at most</label>
              <input
                id="inventory-maxEquipped"
                type="number"
                min="1"
                max="999"
                [attr.class]="'qt-input qt-input-sm w-20 ' + (inherited('maxEquipped') ? 'opacity-60' : '')"
                [value]="inherited('maxEquipped') && parent() ? parent()!.maxEquipped : draft().maxEquipped"
                (input)="set('maxEquipped', toNumber($any($event.target).value))"
              />
              <span class="qt-text-xs qt-text-secondary">at a time, per chat, across objects of the same name</span>
              <ng-container *ngTemplateOutlet="inheritToggle; context: { key: 'maxEquipped' }" />
            </div>
          </div>
        }
        <div class="grid md:grid-cols-2 gap-3">
          <ng-container *ngTemplateOutlet="textField; context: { key: 'emoji', label: 'Emoji glyph', placeholder: isKit() ? '🎒' : '🗡️' }" />
          <ng-container *ngTemplateOutlet="textField; context: { key: 'folder', label: 'Folder', placeholder: isKit() ? 'Loadouts' : 'Weapons/Edged' }" />
        </div>
        <div class="space-y-1">
          <div class="flex items-center justify-between">
            <label class="text-sm text-foreground" for="inventory-svg">SVG glyph</label>
            <ng-container *ngTemplateOutlet="inheritToggle; context: { key: 'svg' }" />
          </div>
          <div class="flex items-start gap-3">
            <textarea
              id="inventory-svg"
              rows="3"
              [attr.class]="'qt-textarea flex-1 font-mono text-xs ' + (inherited('svg') ? 'opacity-60' : '')"
              [value]="shownText('svg')"
              placeholder="<svg viewBox='0 0 24 24'>…</svg>"
              (input)="set('svg', $any($event.target).value)"
            ></textarea>
            <div class="flex flex-col items-center gap-1">
              <span class="inline-flex items-center justify-center w-12 h-12 rounded border qt-border-default qt-bg-muted">
                @if (shownText('svg').trim().startsWith('<svg')) {
                  <img [src]="svgUri(shownText('svg'))" alt="" class="w-10 h-10 object-contain" />
                }
              </span>
              <span [attr.class]="'qt-text-xs ' + (svgBytes() > maxSvgBytes ? 'qt-text-destructive' : 'qt-text-secondary')">{{ svgCounter() }}</span>
            </div>
          </div>
        </div>
        <ng-container
          *ngTemplateOutlet="
            areaField;
            context: {
              key: 'metadataText',
              label: 'Metadata',
              hint: 'Free-form JSON object for anything else (weight, value, charges…).',
              rows: 3,
            }
          "
        />
        @if (item(); as existing) {
          <qt-inventory-pictures-section
            [container]="container()"
            [item]="existing"
            [refreshLists]="actions().refresh"
            (viewingChange)="viewing.set($event)"
          />
        }
      </div>

      <div qt-modal-footer class="flex items-center justify-end gap-2 w-full">
        <button type="button" class="qt-button-secondary qt-button-sm" (click)="closed.emit()" [disabled]="saving()">
          Cancel
        </button>
        <button type="button" class="qt-button-primary qt-button-sm" (click)="save()" [disabled]="saving()">
          {{ saving() ? 'Saving…' : 'Save' }}
        </button>
      </div>
    </qt-modal>

    <ng-template #inheritToggle let-key="key">
      @if (isInstance() && field(key); as f) {
        <label class="flex items-center gap-1 qt-text-xs qt-text-secondary">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="inherit().has(f)"
            (change)="toggleInherit(key, f, $any($event.target).checked)"
            [attr.aria-label]="'Inherit ' + f"
          />
          Inherit
        </label>
      }
    </ng-template>

    <ng-template #textField let-key="key" let-label="label" let-placeholder="placeholder">
      <div class="space-y-1">
        <div class="flex items-center justify-between">
          <label class="text-sm text-foreground" [attr.for]="'inventory-' + key">{{ label }}</label>
          <ng-container *ngTemplateOutlet="inheritToggle; context: { key }" />
        </div>
        <input
          [id]="'inventory-' + key"
          type="text"
          [attr.class]="'qt-input qt-input-sm w-full ' + (inherited(key) ? 'opacity-60' : '')"
          [value]="shownText(key)"
          [placeholder]="placeholder"
          (input)="set(key, $any($event.target).value)"
        />
      </div>
    </ng-template>

    <ng-template #areaField let-key="key" let-label="label" let-hint="hint" let-rows="rows">
      <div class="space-y-1">
        <div class="flex items-center justify-between">
          <label class="text-sm text-foreground" [attr.for]="'inventory-' + key">{{ label }}</label>
          <ng-container *ngTemplateOutlet="inheritToggle; context: { key }" />
        </div>
        <p class="qt-text-xs qt-text-secondary">{{ hint }}</p>
        <textarea
          [id]="'inventory-' + key"
          [rows]="rows"
          [attr.class]="'qt-textarea w-full ' + (inherited(key) ? 'opacity-60' : '')"
          [value]="shownText(key)"
          (input)="set(key, $any($event.target).value)"
        ></textarea>
      </div>
    </ng-template>

    <ng-template #checkField let-key="key" let-label="label" let-hint="hint">
      <div class="flex items-start justify-between gap-2">
        <label class="flex items-start gap-2 text-sm text-foreground">
          <input
            type="checkbox"
            class="qt-checkbox mt-0.5"
            [checked]="shownCheck(key)"
            (change)="set(key, $any($event.target).checked)"
          />
          <span>
            {{ label }}
            <span class="block qt-text-xs qt-text-secondary">{{ hint }}</span>
          </span>
        </label>
        <ng-container *ngTemplateOutlet="inheritToggle; context: { key }" />
      </div>
    </ng-template>
  `,
})
export class InventoryItemEditor {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  /** The tier a new object is created in; an edited one's own tier. */
  readonly container = input.required<InventoryContainer>();
  readonly containerLabel = input.required<string>();
  /** null to create. */
  readonly item = input<ResolvedInventoryItem | null>(null);
  /** What a create makes. */
  readonly createMode = input<'item' | 'kit' | 'archetype'>('item');
  readonly initialFolder = input('');
  /** Everything the tier can reach (kit members, an instance's archetype). */
  readonly poolItems = input<ResolvedInventoryItem[]>([]);
  /** The project whose shelf the pool includes, so a kit's members check against it. */
  readonly projectId = input<string | null>(null);
  readonly actions = input.required<InventoryItemActions>();
  readonly closed = output<void>();
  readonly saved = output<void>();

  protected readonly imageHint = IMAGE_DESCRIPTION_HINT;
  protected readonly maxSvgBytes = INVENTORY_SVG_MAX_BYTES;
  protected readonly svgUri = inventorySvgDataUri;

  protected readonly isKit = computed(() => {
    const item = this.item();
    return item ? item.kind === 'kit' : this.createMode() === 'kit';
  });
  protected readonly isInstance = computed(() => !!this.item()?.parentId);

  protected readonly draft = signal<InventoryDraft>(draftOf(null, 'item', ''));
  protected readonly inherit = signal<ReadonlySet<InventoryInheritableField>>(new Set());
  private readonly touched = signal<ReadonlySet<keyof InventoryDraft>>(new Set());
  protected readonly saving = signal(false);
  /** The Pictures section's full-screen viewer is up (it owns Escape then). */
  protected readonly viewing = signal(false);
  private readonly generalExtra = signal<ResolvedInventoryItem[]>([]);

  protected readonly parent = computed(() => {
    const item = this.item();
    return item?.parentId ? (this.poolItems().find((p) => p.id === item.parentId) ?? null) : null;
  });

  /** v4 `:139-146` — kits, archived objects and duplicates (first wins) excluded. */
  protected readonly memberChoices = computed(() => {
    const seen = new Set<string>();
    return [...this.poolItems(), ...this.generalExtra()].filter((p) => {
      if (p.kind === 'kit' || p.archivedAt || seen.has(p.id)) return false;
      seen.add(p.id);
      return true;
    });
  });

  protected readonly svgBytes = computed(() => svgByteLength(this.draft().svg));
  protected readonly svgCounter = computed(
    () => `${(this.svgBytes() / 1024).toFixed(1)} / ${INVENTORY_SVG_MAX_BYTES / 1024} KB`,
  );

  protected readonly title = computed(() => {
    const item = this.item();
    if (item) return `Edit ${item.title || item.name}`;
    return this.isKit() ? 'New kit' : this.draft().isArchetype ? 'New archetype' : 'New object';
  });

  constructor() {
    // v4's `useState(() => draftOf(…))` / the inherit set's initialiser —
    // seeded once from the inputs the editor mounts with.
    effect(() => {
      const item = this.item();
      const mode = this.createMode();
      const folder = this.initialFolder();
      untracked(() => {
        this.draft.set(draftOf(item, mode, folder));
        if (item?.parentId) {
          const overridden = new Set(item.overriddenFields ?? []);
          this.inherit.set(
            new Set(
              Object.values(DRAFT_FIELD).filter(
                (f): f is InventoryInheritableField => !!f && !overridden.has(f),
              ),
            ),
          );
        } else {
          this.inherit.set(new Set());
        }
        this.touched.set(new Set());
      });
    });
    // A project or group kit may also name Quilltap General's objects (v4 `:135-138`).
    effect(() => {
      const container = this.container();
      const wants = this.isKit() && (container.scope === 'project' || container.scope === 'group');
      untracked(() => {
        if (!wants) {
          this.generalExtra.set([]);
          return;
        }
        void listInventory(this.core, GENERAL_INVENTORY_CONTAINER, { includeArchived: false }).then(
          (items) => this.generalExtra.set(items),
          () => this.generalExtra.set([]),
        );
      });
    });
  }

  protected field(key: keyof InventoryDraft): InventoryInheritableField | undefined {
    return DRAFT_FIELD[key];
  }

  protected inherited(key: keyof InventoryDraft): boolean {
    const field = DRAFT_FIELD[key];
    return this.isInstance() && !!field && this.inherit().has(field);
  }

  /** What the archetype says, shown greyed for an inherited field (v4 `:180-185`). */
  protected parentValue(key: keyof InventoryDraft): string {
    const parent = this.parent();
    if (!parent) return '';
    const value = (draftOf(parent, 'item', '') as unknown as Record<string, unknown>)[key];
    return typeof value === 'string' ? value : String(value ?? '');
  }

  protected shownText(key: keyof InventoryDraft): string {
    return this.inherited(key) ? this.parentValue(key) : String(this.draft()[key]);
  }

  protected shownCheck(key: 'equippedByDefault' | 'canCopy' | 'isArchetype'): boolean {
    const parent = this.parent();
    return this.inherited(key) && parent ? Boolean(parent[key]) : this.draft()[key];
  }

  protected toNumber(value: string): number {
    return Number(value);
  }

  /** v4 `set` (`:148-158`): edit the draft, mark it touched, un-inherit its field. */
  protected set<K extends keyof InventoryDraft>(key: K, value: InventoryDraft[K]): void {
    this.draft.update((d) => ({ ...d, [key]: value }));
    this.touched.update((t) => new Set(t).add(key));
    const field = DRAFT_FIELD[key];
    if (field) {
      this.inherit.update((s) => {
        const next = new Set(s);
        next.delete(field);
        return next;
      });
    }
  }

  /**
   * v4 `toggleInherit` (`:163-174`). Inherit on: the field follows the
   * archetype (its override is cleared on save). Inherit off: the archetype's
   * value becomes the instance's own override, to be edited from there. The
   * parent's value is read BEFORE any write (R-8).
   */
  protected toggleInherit(key: keyof InventoryDraft, field: InventoryInheritableField, on: boolean): void {
    const parent = this.parent();
    const parentDraftValue = parent ? draftOf(parent, 'item', '')[key] : undefined;
    this.inherit.update((s) => {
      const next = new Set(s);
      if (on) next.add(field);
      else next.delete(field);
      return next;
    });
    if (!on) {
      if (parent) this.draft.update((d) => ({ ...d, [key]: parentDraftValue }));
      this.touched.update((t) => new Set(t).add(key));
    }
  }

  protected toggleMember(id: string): void {
    const ids = this.draft().itemIds;
    this.set('itemIds', ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]);
  }

  /** v4 `buildBody` (`:188-226`) — the three shapes; null when the metadata is not an object. */
  buildBody(): InventoryItemBody | null {
    const draft = this.draft();
    const item = this.item();
    const isKit = this.isKit();
    let metadata: Record<string, unknown> | undefined;
    if (draft.metadataText.trim()) {
      try {
        const parsed: unknown = JSON.parse(draft.metadataText);
        if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('not an object');
        metadata = parsed as Record<string, unknown>;
      } catch {
        this.toasts.showError('Metadata must be a JSON object');
        return null;
      }
    }
    const full: InventoryItemBody = {
      name: draft.name.trim(),
      title: orNull(draft.title),
      description: orNull(draft.description),
      imageDescription: orNull(draft.imageDescription),
      equippedByDefault: draft.equippedByDefault,
      maxEquipped: Math.max(1, Math.min(999, Math.round(draft.maxEquipped) || 1)),
      canCopy: draft.canCopy,
      emoji: orNull(draft.emoji),
      svg: orNull(draft.svg),
      metadata: metadata ?? {},
      folder: draft.folder,
    };
    if (!item) {
      return {
        ...full,
        kind: isKit ? 'kit' : 'item',
        isArchetype: !isKit && draft.isArchetype,
        itemIds: isKit ? draft.itemIds : [],
      };
    }
    if (!this.isInstance()) {
      return {
        ...full,
        ...(item.kind !== 'kit' && { isArchetype: draft.isArchetype }),
        ...(isKit && { itemIds: draft.itemIds }),
      };
    }
    // An instance sends only the fields it overrides that were edited here.
    const inherit = this.inherit();
    const touched = this.touched();
    const body: InventoryItemBody = { folder: draft.folder };
    for (const [key, field] of Object.entries(DRAFT_FIELD) as Array<[keyof InventoryDraft, InventoryInheritableField]>) {
      if (inherit.has(field) || !touched.has(key)) continue;
      (body as Record<string, unknown>)[field] = (full as Record<string, unknown>)[field];
    }
    return body;
  }

  /** v4 `save` (`:228-258`). */
  protected async save(): Promise<void> {
    const draft = this.draft();
    const item = this.item();
    if (!draft.name.trim() && !(this.isInstance() && this.inherit().has('name'))) {
      this.toasts.showError('An object needs a name');
      return;
    }
    if (this.svgBytes() > INVENTORY_SVG_MAX_BYTES) {
      this.toasts.showError(`An SVG glyph may be at most ${INVENTORY_SVG_MAX_BYTES / 1024} KB`);
      return;
    }
    const body = this.buildBody();
    if (!body) return;
    const container = this.container();
    const projectId = this.projectId();
    this.saving.set(true);
    try {
      if (!item) {
        await createInventoryItem(this.core, container, body, projectId);
        this.toasts.showSuccess(`"${draft.title || draft.name}" is laid on the table`);
      } else {
        await updateInventoryItem(this.core, container, item.id, body, projectId);
        if (this.isInstance()) {
          const toClear = Array.from(this.inherit()).filter((f) => (item.overriddenFields ?? []).includes(f));
          if (toClear.length > 0) await clearInventoryOverrides(this.core, container, item.id, toClear);
        }
        this.toasts.showSuccess('Saved');
      }
      this.saved.emit();
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, 'The object would not be saved'));
    } finally {
      this.saving.set(false);
      // v4's mutation `onSettled` — the lists, folders and carried reads.
      void this.actions().refresh();
    }
  }

  /** v4 `BaseModal` `closeOnEscape` with `closeOnClickOutside={!saving}` — and not over the viewer. */
  protected onEscape(): void {
    if (this.saving() || this.viewing()) return;
    this.closed.emit();
  }
}
