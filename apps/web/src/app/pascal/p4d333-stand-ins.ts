/**
 * LANE-REMOVE at §S.9 (P4.D334 R-1) — the grammar names P4.D333 lands.
 *
 * P4.D334 is cut from `main` while P4.D333 lands the definition-grammar twins
 * on a sibling branch, so this lane reads the new names through THIS module
 * (identical to their real homes) and the unifier re-points each import:
 *
 * | export                                              | real module                        |
 * |-----------------------------------------------------|------------------------------------|
 * | MAX_WHEN_GROUPS, MAX_INVENTORY_EFFECT_ITEMS,        | `./custom-tool-types`              |
 * | MAX_INVENTORY_REF_LENGTH, InventoryRef,             |                                    |
 * | isInventoryRef, InventoryEffectAction,              |                                    |
 * | inventoryGestureProblems, ToolGateGroup, ToolGate,  |                                    |
 * | WhenObject, When, EffectCondition, EffectWhen,      |                                    |
 * | CustomToolEffect, CustomToolOutcome, gateGroups,    |                                    |
 * | whenGroups, effectWhenGroups                        |                                    |
 * | PlaceholderRef, ScannedPlaceholder, scanPlaceholders,| `./placeholders`                  |
 * | splitInventoryReference                             |                                    |
 * | compareOrdered                                      | `./metadata-match`                 |
 * | ToolGateVerdict, evaluateToolGate                   | `./tool-gate`                      |
 *
 * Types and one-line accessors are transcribed from the pin (`322a5488e`).
 * Anything with BEHAVIOUR the sibling owns answers MAIN's behaviour — which has
 * no inventory, no list rule, no `outcome` placeholder — so a spec whose truth
 * needs the sibling FAILS honestly instead of passing on a copy; every such
 * spec row is gated by `P4D333_SPA_LANDED` (`src/testing/p4d334-gates.ts`).
 *
 * Deletion list: `rg "p4d33[03]-stand-ins|p4d334-gates" apps/web/src`.
 */

import type { ProgressPrimitive } from '../progressions/engine';
import {
  scanPlaceholders as mainScanPlaceholders,
  type PlaceholderRef as MainPlaceholderRef,
  type ScannedPlaceholder as MainScannedPlaceholder,
} from './placeholders';
import type {
  CustomToolEffect as MainCustomToolEffect,
  CustomToolOutcome as MainCustomToolOutcome,
  EffectWhen as MainEffectWhen,
  GateComparator,
  MetadataComparator,
  NumericComparator,
  OutcomeState,
  ParamRef,
  ToolGate as MainToolGate,
  WhenObject as MainWhenObject,
  CustomToolWardrobeGesture,
  QtapCustomTool,
} from './custom-tool-types';
import { evaluateToolGate as mainEvaluateToolGate, type ToolGateVerdict as MainToolGateVerdict } from './tool-gate';

/** Most groups an any-of may carry (v4 `MAX_WHEN_GROUPS`). */
export const MAX_WHEN_GROUPS = 8;
/** Most objects one inventory effect names. */
export const MAX_INVENTORY_EFFECT_ITEMS = 8;
/** Longest object reference, in characters. */
export const MAX_INVENTORY_REF_LENGTH = 200;

/** A `$inventory` reference — `<object>.metadata.<key>` with a typed fallback. */
export interface InventoryRef {
  $inventory: string;
  fallback: number | string | boolean;
}

/** MAIN's behaviour: main has no inventory references, so nothing is one. */
export function isInventoryRef(_value: unknown): _value is InventoryRef {
  return false;
}

export type InventoryEffectAction = 'equip' | 'unequip' | 'pick_up';

/** MAIN's behaviour: main states no inventory rule, so a gesture has no problems. */
export function inventoryGestureProblems(
  _gesture: { action: InventoryEffectAction; equip?: boolean; items?: readonly string[] }
): Array<{ field: 'items' | 'equip'; message: string }> {
  return [];
}

export type InventoryStateValue = 'equipped' | 'carried' | 'reachable' | 'absent';

export interface InventoryStateComparator {
  eq?: InventoryStateValue | ParamRef;
  neq?: InventoryStateValue | ParamRef;
}
export interface GateInventoryStateComparator {
  eq?: InventoryStateValue;
  neq?: InventoryStateValue;
}
export interface GateInventoryCountComparator {
  gt?: number;
  gte?: number;
  lt?: number;
  lte?: number;
  eq?: number;
  neq?: number;
}

/** One AND-group of a gate. */
export interface ToolGateGroup extends MainToolGate {
  inventory?: Record<string, GateInventoryStateComparator>;
  inventoryCount?: Record<string, GateInventoryCountComparator>;
  inventoryAggregate?: Record<string, GateComparator>;
}
/** One group, or an any-of array of groups. */
export type ToolGate = ToolGateGroup | ToolGateGroup[];

export function gateGroups(gate: ToolGate): ToolGateGroup[] {
  return Array.isArray(gate) ? gate : [gate];
}

/** An outcome `when` group, widened with the three inventory subjects. */
export interface WhenObject extends MainWhenObject {
  inventory?: Record<string, InventoryStateComparator>;
  inventoryCount?: Record<string, NumericComparator>;
  inventoryAggregate?: Record<string, MetadataComparator>;
}
export type When = true | WhenObject | WhenObject[];

export function whenGroups(when: When): WhenObject[] {
  if (when === true) return [];
  return Array.isArray(when) ? when : [when];
}

/** An effect condition group, widened with the three inventory subjects. */
export interface EffectWhen extends MainEffectWhen {
  inventory?: Record<string, InventoryStateComparator>;
  inventoryCount?: Record<string, NumericComparator>;
  inventoryAggregate?: Record<string, MetadataComparator>;
}
export type EffectCondition = EffectWhen | EffectWhen[];

export function effectWhenGroups(when: EffectCondition): EffectWhen[] {
  return Array.isArray(when) ? when : [when];
}

export interface CustomToolInventoryGesture {
  action: InventoryEffectAction;
  items: string[];
  equip?: boolean;
}

/** One side effect, widened with the inventory gesture and the log line. */
export type CustomToolEffect = Omit<MainCustomToolEffect, 'when'> & {
  when?: EffectCondition;
  wardrobe?: CustomToolWardrobeGesture;
  inventory?: CustomToolInventoryGesture;
  log?: string;
};

/** One outcome row, widened with the narrator's brief. */
export type CustomToolOutcome = Omit<MainCustomToolOutcome, 'when'> & {
  when: When;
  brief?: string;
};

/** `QtapCustomTool` with the widened outcome rows — for specs that build definitions. */
export type QtapCustomToolAtPin = Omit<QtapCustomTool, 'outcomes' | 'effects' | 'availableWhen' | 'withheldWhen'> & {
  outcomes: CustomToolOutcome[];
  effects?: CustomToolEffect[];
  availableWhen?: ToolGate;
  withheldWhen?: ToolGate;
};

export type { OutcomeState };

/** The placeholder families, widened with the two the pin adds. */
export type PlaceholderRef =
  | MainPlaceholderRef
  | { kind: 'outcome'; field: string }
  | { kind: 'inventory'; ref: string; field: 'title' | 'name' | 'description' | 'state' | 'metadata'; key?: string };
export interface ScannedPlaceholder extends Omit<MainScannedPlaceholder, 'ref'> {
  ref: PlaceholderRef;
}

/** MAIN's behaviour: `{{outcome.*}}` and `{{inventory.*}}` classify `unknown`. */
export const scanPlaceholders: (text: string) => ScannedPlaceholder[] = mainScanPlaceholders;

/** MAIN's behaviour: no inventory placeholder family exists, so nothing splits. */
export function splitInventoryReference(
  _rest: string
): { ref: string; field: 'title' | 'name' | 'description' | 'state' | 'metadata'; key?: string } | null {
  return null;
}

/** MAIN's behaviour: no ordered comparison is exported, so every ordering declines. */
export function compareOrdered(_key: 'gt' | 'gte' | 'lt' | 'lte', _subject: number, _operand: number): boolean {
  return false;
}

export interface ToolGateVerdict extends MainToolGateVerdict {
  /** The zero-based index of the group that decided an any-of clause. */
  heldGroup?: number;
}

/**
 * MAIN's behaviour behind the pin's four-argument signature: the inventory
 * context is accepted and ignored (main's gate declines every inventory record
 * it cannot see), and an array clause is read as main reads any object.
 */
export function evaluateToolGate(
  definition: { availableWhen?: ToolGate; withheldWhen?: ToolGate },
  metadata: Record<string, unknown> | null | undefined,
  progress?: Record<string, ProgressPrimitive> | null,
  _inventory?: unknown
): ToolGateVerdict {
  return mainEvaluateToolGate(
    definition as unknown as Parameters<typeof mainEvaluateToolGate>[0],
    metadata,
    progress
  );
}
