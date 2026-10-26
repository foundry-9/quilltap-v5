/**
 * LANE-REMOVE at §S.9 (P4.D334 R-1) — the Inventory SPA names P4.D330 lands.
 *
 * P4.D334 reads these through THIS module (names identical to their real
 * homes) and the unifier re-points each import:
 *
 * | export                                                   | real module                                   |
 * |----------------------------------------------------------|-----------------------------------------------|
 * | ResolvedInventoryItem, MockInventoryRow, InventoryOrigin | `../../core/core-contract`                    |
 * | inventoryDisplayName, inventoryLabel, isKin, kitState    | `../../inventory/inventory-display`           |
 * | inventoryOriginLabel                                     | `../../inventory/inventory-container`         |
 * | InventoryPicture                                         | `../../inventory/inventory-glyph`             |
 * | InventoryReferencePicker, inventoryPickRule,             | `../../inventory/inventory-reference-picker`  |
 * | InventoryReferenceForm, InventoryPickAction              |                                               |
 * | InventoryVantageItemsLoader, InventoryVantagePool        | `../../inventory/inventory-vantage-items`     |
 * | inventoryReferenceLines, CustomToolInventoryLines        | `../../chat/custom-tool-inventory-lines`      |
 * | ReferencesAtPin                                          | the unifier adds `inventoryAggregates?` to    |
 * |                                                          | `chat/custom-tools.api.ts` (R-10) and the     |
 * |                                                          | alias is deleted                              |
 * | asPreviewRequest, asAuditRequest                         | deleted when `CustomToolPreviewRequest` /     |
 * |                                                          | `CustomToolAuditRequest` carry `inventory?`   |
 * |                                                          | (R-12)                                        |
 *
 * Types and one-line accessors are transcribed; anything with BEHAVIOUR P4.D330
 * owns answers MAIN's behaviour (nothing loads, nothing renders, no rule
 * refuses), so a spec whose truth needs the sibling fails honestly — every such
 * row is gated by `P4D330_SPA_LANDED` (`src/testing/p4d334-gates.ts`).
 *
 * Deletion list: `rg "p4d33[03]-stand-ins|p4d334-gates" apps/web/src`.
 */

import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import type { CustomToolReferences } from '../../chat/custom-tools.api';
import type { CoreClient } from '../../core/core-client';
import type { CustomToolAuditRequest, CustomToolPreviewRequest } from '../../core/core-contract';
import type { WardrobeVantage } from '../../pascal/tool-draft';

export type InventoryKind = 'item' | 'kit';

/** Where an object lives, as a collection read tags it. */
export interface InventoryOrigin {
  scope: 'character' | 'group' | 'project' | 'general';
  id: string | null;
  name: string;
}

/** An object as every reader outside the writer sees it (the fields the Workbench reads). */
export interface ResolvedInventoryItem {
  id: string;
  kind: InventoryKind;
  name: string;
  title?: string | null;
  description?: string | null;
  isArchetype: boolean;
  parentId: string | null;
  itemIds: string[];
  emoji?: string | null;
  svg?: string | null;
  imageFileId?: string | null;
  metadata: Record<string, unknown>;
  folder: string;
  archivedAt?: string | null;
  origin?: InventoryOrigin;
}

/** One row of the Workbench's mock inventory — the preview / audit body's `inventory` key. */
export interface MockInventoryRow {
  ref: string;
  state: 'equipped' | 'carried' | 'reachable' | 'absent';
  count?: number;
  metadata?: Record<string, unknown>;
}

/** MAIN's behaviour: the name alone. */
export function inventoryDisplayName(item: Pick<ResolvedInventoryItem, 'name' | 'title'>): string {
  return item.name;
}

/** `title?.trim() || name` (a one-line accessor, v4 `inventoryLabel`). */
export function inventoryLabel(item: Pick<ResolvedInventoryItem, 'name' | 'title'>): string {
  return item.title?.trim() || item.name;
}

/** Kin share a resolved name, case-insensitively (a one-line accessor, v4 `isKin`). */
export function isKin(a: Pick<ResolvedInventoryItem, 'name'>, b: Pick<ResolvedInventoryItem, 'name'>): boolean {
  return a.name.trim().toLowerCase() === b.name.trim().toLowerCase();
}

/** MAIN's behaviour: no kit is ever carried. */
export function kitState(
  _kit: Pick<ResolvedInventoryItem, 'itemIds'>,
  _equipped: readonly Pick<ResolvedInventoryItem, 'id' | 'parentId'>[],
  _resolvable: (id: string) => Pick<ResolvedInventoryItem, 'isArchetype' | 'kind' | 'archivedAt'> | undefined
): 'all' | 'some' | 'none' {
  return 'none';
}

/** MAIN's behaviour: no origin is named. */
export function inventoryOriginLabel(_origin: InventoryOrigin | null | undefined): string | null {
  return null;
}

export type InventoryPickAction = 'equip' | 'unequip' | 'pick_up';
export type InventoryReferenceForm = 'label' | 'id';

/** MAIN's behaviour: no rule refuses. */
export function inventoryPickRule(
  _item: Pick<ResolvedInventoryItem, 'isArchetype'>,
  _action: InventoryPickAction
): string | null {
  return null;
}

/** The object's picture — renders nothing until P4.D330 lands the real one. */
@Component({
  selector: 'qt-inventory-picture',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: ``,
})
export class InventoryPicture {
  readonly item = input.required<Pick<ResolvedInventoryItem, 'imageFileId' | 'name' | 'title'>>();
  readonly size = input(48);
}

/** The reference picker — renders nothing until P4.D330 lands the real one. */
@Component({
  selector: 'qt-inventory-reference-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: ``,
})
export class InventoryReferencePicker {
  readonly open = input(false);
  readonly vantage = input.required<WardrobeVantage>();
  readonly action = input.required<InventoryPickAction>();
  readonly highlightId = input<string | null>(null);
  readonly reference = input<InventoryReferenceForm>('label');
  readonly referenceChange = output<InventoryReferenceForm>();
  readonly picked = output<{ item: ResolvedInventoryItem; reference: InventoryReferenceForm }>();
  readonly closed = output<void>();
}

/** v4 `ResolvedInventoryPool` as the loader answers it. */
export interface InventoryVantagePool {
  items: ResolvedInventoryItem[];
  owns: (id: string) => boolean;
}

/** MAIN's behaviour: nothing loads. */
export class InventoryVantageItemsLoader {
  readonly items = signal<ResolvedInventoryItem[]>([]);
  readonly loading = signal(false);
  readonly fetched = signal(false);
  readonly pool = computed<InventoryVantagePool>(() => ({ items: this.items(), owns: () => true }));

  constructor(_core: CoreClient) {}

  async load(_vantage: WardrobeVantage | null): Promise<void> {
    this.items.set([]);
    this.loading.set(false);
    this.fetched.set(false);
  }
}

export interface InventoryReferenceLine {
  key: 'inventoryReads' | 'equips' | 'unequips' | 'picksUp';
  lead: string;
  values: string[];
  tail: string;
}

/** MAIN's behaviour: an older server's roster — no inventory sentences. */
export function inventoryReferenceLines(_refs: CustomToolReferences | undefined): InventoryReferenceLine[] {
  return [];
}

/** The inventory sentences — renders nothing until P4.D330 lands the real one. */
@Component({
  selector: 'qt-custom-tool-inventory-lines',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: ``,
})
export class CustomToolInventoryLines {
  readonly references = input<CustomToolReferences | undefined>();
}

/**
 * `CustomToolReferences` with the five inventory keys v4's `CustomToolRunDialog`
 * carries at the pin. P4.D330 edits the interface IN PLACE with four of them
 * (`inventoryReads`, `equips`, `unequips`, `picksUp`); the fifth,
 * `inventoryAggregates`, is the unifier's addition (R-10). Until then this alias
 * reads all five, and it is deleted with the stand-in.
 */
export type ReferencesAtPin = CustomToolReferences & {
  inventoryReads?: string[];
  inventoryAggregates?: string[];
  equips?: string[];
  unequips?: string[];
  picksUp?: string[];
};

/** The preview request with the `inventory` key P4.D330 adds in place (R-12). */
export function asPreviewRequest(
  request: Omit<CustomToolPreviewRequest, 'inventory'> & { inventory?: MockInventoryRow[] }
): CustomToolPreviewRequest {
  return request as CustomToolPreviewRequest;
}

/** The audit request with the `inventory` key P4.D330 adds in place (R-12). */
export function asAuditRequest(
  request: Omit<CustomToolAuditRequest, 'inventory'> & { inventory?: MockInventoryRow[] }
): CustomToolAuditRequest {
  return request as CustomToolAuditRequest;
}
