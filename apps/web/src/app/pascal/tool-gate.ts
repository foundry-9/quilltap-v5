/**
 * The availability gate — whether a tool is dealt to this invoker at all (v4
 * `lib/pascal/tool-gate.ts`).
 *
 * An outcome table decides what happens once a tool is run. A gate decides
 * something earlier and coarser: whether the tool appears on the roster in the
 * first place. A definition that declares `availableWhen` is offered only to an
 * invoker whose fact sheet satisfies it; one that declares `withheldWhen` is
 * kept from an invoker whose sheet satisfies THAT. A definition declaring
 * neither is offered to everyone, which is every file written before this key
 * existed.
 *
 * ## Why metadata and progress alone
 *
 * The gate is answered before the deal. There is no roll to test, no resolved
 * parameters (nobody has called anything yet), and no consult. What a character
 * carries into the room is their `metadata.json` — and, derived from the
 * `progressions` key inside it against the wall clock, their timed
 * progressions. Those two are the only subjects a pre-roll test can honestly
 * have, and the reason a gate's operands are literals rather than
 * `$param`/`$state` references.
 *
 * The progress sheet is what makes `availableWhen: { progress: {
 * "cannon.complete": { eq: true } } }` — the whole weapon-recharge use case —
 * a gate rather than a branch in every outcome table. The roster already holds
 * the invoker's metadata, so deriving the sheet from it adds no reads.
 *
 * ## Fail-soft, and therefore fail-CLOSED for `availableWhen`
 *
 * Metadata tests never throw: an absent key, a list, a value of the wrong type
 * all simply fail to match (see {@link metadataComparatorHolds}). Read through
 * the gate, that means a character with no sheet at all fails every
 * `availableWhen` — the tool is withheld — and satisfies no `withheldWhen` — the
 * tool is offered. Both are the safe reading of "we could not establish that
 * this character qualifies".
 *
 * ## Any-of
 *
 * A clause is one group (every test must hold) or an array of groups (any
 * group may hold). AND binds tighter than OR; groups are tried left to right
 * and the first to hold decides. An empty context declines every group, so
 * the fail-closed / fail-open reading below holds for an array unchanged (v4
 * `a53024d3c`).
 *
 * ## Inventory, as data only
 *
 * v4 gates may also test the invoker's Inventory (`inventory`,
 * `inventoryCount`, `inventoryAggregate`). The browser has no Inventory
 * context yet, so this port takes v4's fourth argument only in its NULL form —
 * under which v4 declines every Inventory test (`inventoryStateHolds` /
 * `inventoryCountHolds` / `inventoryAggregateHolds` all answer false for a
 * missing bag). A group carrying any non-empty Inventory record therefore
 * never holds here, exactly as it never holds in v4 with no bag.
 *
 * CLIENT-SAFE: pure, and imports only client-safe modules, so the Workbench can
 * tell an author whether a fact sheet would be dealt to without a round trip.
 */

import type { ProgressPrimitive } from '../progressions/engine';
import { gateGroups, type QtapCustomTool, type ToolGate, type ToolGateGroup } from './custom-tool-types';
import { metadataComparatorHolds, type Primitive } from './metadata-match';

/** What a gate decided, and which clause decided it. */
export interface ToolGateVerdict {
  /** Whether this invoker may be offered the tool. */
  available: boolean;
  /** The clause that withheld it. Absent whenever the tool is available. */
  withheldBy?: 'availableWhen' | 'withheldWhen';
  /**
   * For an any-of clause (an array), the index of the group that held — the
   * one that opened an `availableWhen` or closed a `withheldWhen`. Absent
   * when no group held, and for a bare-object clause, where there is only
   * the one.
   */
  heldGroup?: number;
}

/** A definition carries a gate when it declares either clause. */
export function hasToolGate(
  definition: Pick<QtapCustomTool, 'availableWhen' | 'withheldWhen'>,
): boolean {
  return definition.availableWhen !== undefined || definition.withheldWhen !== undefined;
}

/**
 * Evaluate a definition's gate against one invoker's fact sheet.
 *
 * Total: an ungated definition is available, and no input can make this throw.
 */
export function evaluateToolGate(
  definition: Pick<QtapCustomTool, 'availableWhen' | 'withheldWhen'>,
  metadata: Record<string, unknown> | null | undefined,
  /**
   * The invoker's derived progress sheet (`flattenProgressions`), keyed
   * `"<id>.<field>"`. Omitted → `{}`, so every `progress` test fails and the
   * fail-closed / fail-open reading above applies to it unchanged.
   */
  progress?: Record<string, ProgressPrimitive> | null,
  /**
   * v4's Inventory context for this chat. The browser has none yet, so only
   * the absent form is admitted: every `inventory` / `inventoryCount` /
   * `inventoryAggregate` test declines (see the module note).
   */
  inventory: null = null,
): ToolGateVerdict {
  const sheet = metadata ?? {};
  const progressSheet = progress ?? {};
  const bag = inventory;

  let opened: number | undefined;
  if (definition.availableWhen) {
    const held = heldGroupOf(definition.availableWhen, sheet, progressSheet, bag);
    if (held === undefined) return { available: false, withheldBy: 'availableWhen' };
    if (Array.isArray(definition.availableWhen)) opened = held;
  }

  if (definition.withheldWhen) {
    const held = heldGroupOf(definition.withheldWhen, sheet, progressSheet, bag);
    if (held !== undefined) {
      return Array.isArray(definition.withheldWhen)
        ? { available: false, withheldBy: 'withheldWhen', heldGroup: held }
        : { available: false, withheldBy: 'withheldWhen' };
    }
  }

  return opened === undefined ? { available: true } : { available: true, heldGroup: opened };
}

/** The first group of a clause that holds, left to right; undefined when none does. */
function heldGroupOf(
  clause: ToolGate,
  metadata: Record<string, unknown>,
  progress: Record<string, ProgressPrimitive>,
  inventory: null,
): number | undefined {
  const index = gateGroups(clause).findIndex((group) => gateHolds(group, metadata, progress, inventory));
  return index >= 0 ? index : undefined;
}

/** Every test in a gate group must hold. Keys AND together, as they do in a `when`. */
function gateHolds(
  gate: ToolGateGroup,
  metadata: Record<string, unknown>,
  progress: Record<string, ProgressPrimitive>,
  inventory: null,
): boolean {
  // Both subjects go through the SAME comparison table — one semantics, no
  // second implementation to drift — differing only in which sheet they read.
  for (const [key, comparator] of Object.entries(gate.metadata ?? {})) {
    const holds = metadataComparatorHolds(comparator, key, metadata, {
      // A gate's operands are literals by construction — the schema admits no
      // `$param` or `$state` here — so resolution is the identity.
      resolveOperand: (comparatorKey) => comparator[comparatorKey] as Primitive,
    });
    if (!holds) return false;
  }

  for (const [key, comparator] of Object.entries(gate.progress ?? {})) {
    const holds = metadataComparatorHolds(comparator, key, progress, {
      resolveOperand: (comparatorKey) => comparator[comparatorKey] as Primitive,
    });
    if (!holds) return false;
  }

  // v4 reads these three against the invoker's bag; with no bag (`null`, the
  // only form this port admits) every one of v4's three holds-functions
  // declines, so a group naming any Inventory test cannot hold. An EMPTY
  // record tests nothing and is skipped, as v4's loop over no entries is.
  for (const subject of ['inventory', 'inventoryCount', 'inventoryAggregate'] as const) {
    if (Object.keys(gate[subject] ?? {}).length > 0 && inventory === null) return false;
  }

  return true;
}

/** True when a gate clause tests the progress sheet in any group — the roster derives it only then. */
export function gateNamesProgress(gate: ToolGate | undefined): boolean {
  return !!gate && gateGroups(gate).some((group) => Object.keys(group.progress ?? {}).length > 0);
}
