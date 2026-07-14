import type { CoreClient } from '../../../core/core-client';
import type {
  TextReplacementRule,
  TextReplacementRuleInput,
} from '../../../core/core-contract';

/**
 * The settings text-replacements client surface (lane A's pinned dispatch verbs
 * over `/api/v1/settings/text-replacements`). The five request shapes live in
 * core-contract's P4.6al block and ride the CoreRequest union (folded at
 * unification).
 *
 * @module screens/settings/chat/text-replacements.api
 */

export interface TextReplacementList {
  rules: TextReplacementRule[];
  count: number;
}

export async function listTextReplacements(core: CoreClient): Promise<TextReplacementList> {
  const data = await core.dispatchData({ type: 'textReplacementsList' });
  return {
    rules: (data['rules'] as TextReplacementRule[] | undefined) ?? [],
    count: (data['count'] as number | undefined) ?? 0,
  };
}

export async function createTextReplacement(
  core: CoreClient,
  input: TextReplacementRuleInput,
): Promise<TextReplacementRule> {
  const data = await core.dispatchData({
    type: 'textReplacementCreate',
    ...input,
  });
  return data as unknown as TextReplacementRule;
}

export async function updateTextReplacement(
  core: CoreClient,
  id: string,
  input: Partial<TextReplacementRuleInput>,
): Promise<TextReplacementRule> {
  const data = await core.dispatchData({
    type: 'textReplacementUpdate',
    id,
    ...input,
  });
  return data as unknown as TextReplacementRule;
}

export async function deleteTextReplacement(core: CoreClient, id: string): Promise<void> {
  await core.dispatchData({ type: 'textReplacementDelete', id });
}

export async function bulkReplaceTextReplacements(
  core: CoreClient,
  rules: TextReplacementRuleInput[],
): Promise<TextReplacementList> {
  const data = await core.dispatchData({
    type: 'textReplacementsBulkReplace',
    rules,
  });
  return {
    rules: (data['rules'] as TextReplacementRule[] | undefined) ?? [],
    count: (data['count'] as number | undefined) ?? 0,
  };
}
