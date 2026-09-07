/**
 * Pure apply-plan builder for the optimizer's Apply step — the field-routing
 * half of v4 `useCharacterOptimizer.ts:285-436` (`getAcceptedChanges` +
 * `applyChanges`'s routing/merge logic), split out from the actual dispatch
 * fan-out so it is independently, deterministically testable (the parity spec
 * drives it against a recorded suggestion set, mutation-proven).
 */

import type { CharacterDetail } from '../../../../core/core-contract';
import type { OptimizerSuggestion, SuggestionDecision } from '../detail-generators.api';

export interface AcceptedChange {
  suggestion: OptimizerSuggestion;
  finalValue: string;
}

/** v4 `getAcceptedChanges` (`:285-301`). */
export function getAcceptedChanges(
  suggestions: readonly OptimizerSuggestion[],
  decisions: ReadonlyMap<string, SuggestionDecision>,
  editedValues: ReadonlyMap<string, string>,
): AcceptedChange[] {
  return suggestions
    .filter((s) => {
      const decision = decisions.get(s.id);
      return decision === 'accepted' || decision === 'edited';
    })
    .map((s) => ({
      suggestion: s,
      finalValue:
        decisions.get(s.id) === 'edited' ? (editedValues.get(s.id) ?? s.proposedValue) : s.proposedValue,
    }));
}

/** v4 `applyChanges` `:327-333` — fields that map 1:1 onto the PUT body. */
const SIMPLE_FIELDS = ['identity', 'description', 'manifesto', 'personality', 'exampleDialogues'];

/** v4 `applyChanges` `:335-342` — valid physicalDescription sub-field keys. */
const PHYSICAL_KEYS = [
  'fullDescription',
  'headAndShouldersPrompt',
  'shortPrompt',
  'mediumPrompt',
  'longPrompt',
  'completePrompt',
];

export interface OptimizerApplyPlan {
  /** Simple scalars + talkativeness, ready to send verbatim. */
  updatePayload: Record<string, unknown>;
  scenarioUpdates: Array<{ subId: string; finalValue: string }>;
  physicalUpdates: Array<{ key: string; finalValue: string }>;
  promptUpdates: Array<{ subId: string; finalValue: string }>;
  promptCreates: Array<{ name: string; finalValue: string }>;
  wardrobeUpdates: Array<{ subId: string; finalValue: string }>;
  wardrobeCreates: Array<{ suggestion: OptimizerSuggestion; finalValue: string }>;
  aliasAdditions: string[];
}

/** v4 `applyChanges` `:356-393` — routes each accepted change by `suggestion.field`. */
export function buildOptimizerApplyPlan(accepted: readonly AcceptedChange[]): OptimizerApplyPlan {
  const plan: OptimizerApplyPlan = {
    updatePayload: {},
    scenarioUpdates: [],
    physicalUpdates: [],
    promptUpdates: [],
    promptCreates: [],
    wardrobeUpdates: [],
    wardrobeCreates: [],
    aliasAdditions: [],
  };

  for (const { suggestion, finalValue } of accepted) {
    const field = suggestion.field;
    if (SIMPLE_FIELDS.includes(field)) {
      plan.updatePayload[field] = finalValue;
    } else if (field === 'talkativeness') {
      const n = parseFloat(finalValue);
      if (!Number.isNaN(n)) {
        plan.updatePayload['talkativeness'] = Math.min(1, Math.max(0.1, n));
      }
    } else if (field === 'scenarios') {
      // New scenarios are no longer proposed; only refinements (with a subId) apply.
      if (suggestion.subId) {
        plan.scenarioUpdates.push({ subId: suggestion.subId, finalValue });
      }
    } else if (field === 'physicalDescription') {
      const key = suggestion.subId;
      if (key && PHYSICAL_KEYS.includes(key)) {
        plan.physicalUpdates.push({ key, finalValue });
      }
    } else if (field === 'systemPrompt') {
      if (suggestion.subId) {
        plan.promptUpdates.push({ subId: suggestion.subId, finalValue });
      } else {
        const name = suggestion.name?.trim() || suggestion.title?.trim() || 'Refined Prompt';
        plan.promptCreates.push({ name, finalValue });
      }
    } else if (field === 'wardrobeItems') {
      if (suggestion.subId) {
        plan.wardrobeUpdates.push({ subId: suggestion.subId, finalValue });
      } else if (suggestion.wardrobeItem) {
        plan.wardrobeCreates.push({ suggestion, finalValue });
      }
    } else if (field === 'aliases') {
      const alias = finalValue.trim();
      if (alias) plan.aliasAdditions.push(alias);
    }
  }

  return plan;
}

/** The subset of a character record the plan's array/object merges need. */
export type MergeableCharacter = Pick<CharacterDetail, 'aliases' | 'scenarios' | 'physicalDescription'>;

/**
 * v4 `applyChanges` `:395-436` — merges alias/scenario/physicalDescription
 * refinements into their FULL current value (the PUT body replaces each
 * wholesale), producing the final `updatePayload` to hand to
 * `applyCharacterFieldUpdates`'s `mainUpdates`. `nowIso` is injected so the
 * scenario `updatedAt` stamp is deterministic under test.
 */
export function mergeOptimizerApplyPlan(
  plan: OptimizerApplyPlan,
  character: MergeableCharacter,
  nowIso: string = new Date().toISOString(),
): Record<string, unknown> {
  const updatePayload = { ...plan.updatePayload };

  if (plan.aliasAdditions.length > 0) {
    const existingAliases = (character.aliases ?? []).slice();
    const seen = new Set(existingAliases.map((a) => a.toLowerCase()));
    for (const alias of plan.aliasAdditions) {
      if (!seen.has(alias.toLowerCase())) {
        existingAliases.push(alias);
        seen.add(alias.toLowerCase());
      }
    }
    updatePayload['aliases'] = existingAliases;
  }

  if (plan.scenarioUpdates.length > 0) {
    const existingScenarios = character.scenarios ?? [];
    updatePayload['scenarios'] = existingScenarios.map((s) => {
      const update = plan.scenarioUpdates.find((u) => u.subId === s['id']);
      return update ? { ...s, content: update.finalValue, updatedAt: nowIso } : s;
    });
  }

  if (plan.physicalUpdates.length > 0) {
    const existingPhysical = character.physicalDescription ?? null;
    const merged: Record<string, unknown> = existingPhysical
      ? ({ ...existingPhysical } as Record<string, unknown>)
      : { name: 'Appearance' };
    for (const { key, finalValue } of plan.physicalUpdates) {
      merged[key] = finalValue;
    }
    if (typeof merged['name'] !== 'string' || (merged['name'] as string).trim() === '') {
      merged['name'] = 'Appearance';
    }
    updatePayload['physicalDescription'] = merged;
  }

  return updatePayload;
}
