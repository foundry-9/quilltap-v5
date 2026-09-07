/**
 * Persist wizard-generated content that the wizard's own `onApply` does NOT
 * write into host form state — v4's `app/aurora/shared/save-generated-*.ts`
 * trio, shared by `NewCharacterView` and `CharacterEditView`, transcribed at
 * the `f699da6f6` pin. Every call here targets an EXISTING v5 verb
 * (`characterUpdate`, `characterWardrobeCreate`, `characterScenarioCreate`) —
 * none of these are §B-scoped or sibling-owned.
 *
 * @module screens/characters/generators/wizard/save-generated
 */

import { CoreClient, coreErrorMessage } from '../../../../core/core-client';
import type { CharacterScenario } from '../../../../core/core-contract';
import type { ToastService } from '../../../../ui/toast.service';
import type { GeneratedPhysicalDescription, GeneratedWardrobeItem } from '../edit-generators.api';
import { orderGeneratedItemsLeafFirst } from './wizard-wardrobe';

/**
 * PUT the character's physical description (v4
 * `save-generated-physical-description.ts`). The cutover collapsed the
 * multi-record array to one record on the character row; `characterUpdate`
 * PATCHes it directly and the repository's write overlay routes it into the
 * vault. Toasts internally on both outcomes (v4's shared helper does, not
 * the callers) — `failureMessage` is the two v4 hosts' differently-worded
 * fallback when the server sent no `error`.
 */
export async function saveGeneratedPhysicalDescription(
  core: CoreClient,
  toasts: ToastService,
  characterId: string,
  pd: GeneratedPhysicalDescription,
  failureMessage: string,
): Promise<void> {
  try {
    await core.dispatchData({
      type: 'characterUpdate',
      characterId,
      character: {
        physicalDescription: {
          name: pd.name,
          headAndShouldersPrompt: pd.headAndShouldersPrompt,
          shortPrompt: pd.shortPrompt,
          mediumPrompt: pd.mediumPrompt,
          longPrompt: pd.longPrompt,
          completePrompt: pd.completePrompt,
          fullDescription: pd.fullDescription,
        },
      },
    });
    toasts.showSuccess('Physical description saved');
  } catch (err) {
    toasts.showError(coreErrorMessage(err, failureMessage));
  }
}

/**
 * Persist wizard-generated wardrobe items (v4 `save-generated-wardrobe.ts`):
 * leaf garments first, then composites with their component titles resolved
 * to the ids the API minted. Survives individual item failures (v4 logs and
 * continues); returns how many saved and how many were composite outfits.
 */
export async function saveGeneratedWardrobeItems(
  core: CoreClient,
  characterId: string,
  items: GeneratedWardrobeItem[],
): Promise<{ saved: number; outfits: number }> {
  const ordered = orderGeneratedItemsLeafFirst(items);
  const idByTitle = new Map<string, string>();
  let saved = 0;
  let outfits = 0;

  for (const item of ordered) {
    const componentItemIds = (item.components ?? [])
      .map((title) => idByTitle.get(title.trim().toLowerCase()))
      .filter((id): id is string => Boolean(id));

    try {
      const data = await core.dispatchData({
        type: 'characterWardrobeCreate',
        characterId,
        item: {
          title: item.title,
          description: item.description || null,
          imagePrompt: item.imagePrompt || null,
          types: item.types,
          appropriateness: item.appropriateness || null,
          isDefault: item.isDefault === true,
          componentItemIds,
          replace: item.replace === true,
        },
      });
      saved++;
      if (componentItemIds.length > 0) outfits++;
      const createdId = (data['item'] as { id?: unknown } | undefined)?.id;
      if (typeof createdId === 'string') {
        idByTitle.set(item.title.trim().toLowerCase(), createdId);
      }
    } catch {
      // v4 logs and continues — one item's failure doesn't abort the batch.
    }
  }

  return { saved, outfits };
}

/**
 * Persist wizard-generated scenarios, one `characterScenarioCreate` per
 * entry, surviving individual failures (v4 `save-generated-scenarios.ts`).
 * Returns the server-minted rows so the edit host can splice them into form
 * state without a refetch (v4 `:154`).
 */
export async function saveGeneratedScenarios(
  core: CoreClient,
  characterId: string,
  scenarios: Array<{ title: string; content: string }>,
): Promise<{ saved: number; scenarios: CharacterScenario[] }> {
  let saved = 0;
  const savedScenarios: CharacterScenario[] = [];

  for (const scenario of scenarios) {
    try {
      const data = await core.dispatchData({
        type: 'characterScenarioCreate',
        characterId,
        title: scenario.title,
        content: scenario.content,
      });
      const row = data['scenario'] as Record<string, unknown> | undefined;
      if (row) {
        saved++;
        savedScenarios.push({
          id: row['id'] as string,
          title: row['title'] as string,
          content: row['content'] as string,
          createdAt: row['createdAt'] as string,
          updatedAt: row['updatedAt'] as string,
        });
      }
    } catch {
      // v4 logs and continues.
    }
  }

  return { saved, scenarios: savedScenarios };
}
