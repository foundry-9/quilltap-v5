import type { CoreClient } from '../core/core-client';
import type {
  EffectPin,
  InterventionDecision,
  InterventionOverrides,
  RunSpan,
} from '../core/core-contract';
import type {
  CustomToolEffect,
  CustomToolParameter,
  InterventionFieldKey,
  OutcomeState,
  Visibility,
  When,
} from '../pascal/custom-tool-types';

/**
 * Pascal operator intervention — the hold dialog's wire (v4 `839575d89`).
 *
 * `InterventionWire` is P4.D345's (its order's "Contract `InterventionWire`"
 * block is the BINDING copy): the four verbs `chatCustomToolInterventionsList`
 * / `…Preview` / `…Reask` / `…Resolve` and these shapes, named as v4's
 * `lib/pascal/intervention/view.ts` / `overrides.ts` name them. Every body is
 * read STRUCTURALLY through `dispatchData` (the `custom-tools.api.ts`
 * precedent) — no response variant is pinned by the SPA. The client never
 * computes a run itself: every edit is a set of PINS sent to the preview verb,
 * and the server's recompute is what is shown — and what is finally dealt.
 *
 * @module chat/custom-tool-interventions.api
 */

export type { EffectPin, InterventionDecision, InterventionOverrides, RunSpan };

/** One declared effect, as this run resolved it. */
export interface InterventionEffectView {
  index: number;
  kind: 'write' | 'wardrobe' | 'inventory' | 'log';
  /** What the effect touches: a write's target, or the gesture's action. */
  label: string;
  /** Whether it fires in this run. */
  fired: boolean;
  /** Why it does not, when it does not. */
  skipped?: string;
  /** A write's resolved value. */
  value?: number | string | boolean;
  /** A gesture's resolved references. */
  items?: string[];
  /** A log line, rendered. */
  log?: string;
}

/** A run, as the dialog shows it. */
export interface InterventionRunView {
  params: Record<string, number | string | boolean>;
  rollForm: 'range' | 'dice';
  notation?: string;
  raw: number;
  diceRolls?: number[];
  diceBreakdown: string;
  /** What the table could have drawn under these parameters (dice: the total). */
  rollSpan: RunSpan;
  value: number;
  /** What the value could have come to after modifiers (dice: the total). */
  valueSpan: RunSpan;
  outcomeIndex: number;
  /** When the outcome is forced: the row the table itself would pick. */
  tableOutcomeIndex?: number;
  state: OutcomeState;
  message: string;
  chipLabel?: string;
  title: string;
  visibility: Visibility;
  announcer: 'pascal' | 'character';
  llm?: { ok: boolean; output: string; prompt: string };
  effects: InterventionEffectView[];
}

/** The result of a preview, Re-ask or resolve recompute. */
export interface InterventionPreview {
  run: InterventionRunView;
  staleAnswer: boolean;
  /** The prompt a Re-ask would pose now. */
  promptNow?: string;
  clampedParams: string[];
  warnings: string[];
}

/** One held run, as `chatCustomToolInterventionsList` lists it. */
export interface InterventionHoldView {
  id: string;
  tool: string;
  title: string;
  characterName: string | null;
  entrance: 'llm' | 'trigger';
  triggerPhrase?: string;
  createdAt: number;
  /** What the dialog may show and an override may name. */
  eligible: { fields: InterventionFieldKey[]; params: string[]; effects: number[] };
  /** Whether a character announcer is possible (a run somebody made). */
  hasCharacter: boolean;
  /** The operator's own file: the odds are shown whatever `revealOdds` says. */
  definition: {
    parameters: Record<string, CustomToolParameter>;
    outcomes: Array<{ when: When; state: OutcomeState; message: string; brief?: string }>;
    effects: CustomToolEffect[];
    revealOdds: boolean;
    hasLlm: boolean;
    diceSides?: number;
  };
  baseline: InterventionPreview;
  /** Every consult so far — the original, then one per Re-ask. */
  consults: Array<{ ok: boolean; output: string; prompt: string }>;
}

/** The chat's pending holds, oldest first. */
export async function fetchInterventions(
  core: CoreClient,
  chatId: string,
): Promise<{ holds: InterventionHoldView[] }> {
  const data = await core.dispatchData({ type: 'chatCustomToolInterventionsList', chatId });
  return data as unknown as { holds: InterventionHoldView[] };
}

/** Recompute a held run under the operator's pins. */
export async function previewIntervention(
  core: CoreClient,
  chatId: string,
  id: string,
  overrides: InterventionOverrides,
): Promise<{ preview: InterventionPreview }> {
  const data = await core.dispatchData({ type: 'chatCustomToolInterventionPreview', chatId, id, overrides });
  return data as unknown as { preview: InterventionPreview };
}

/** Ask the oracle again under the pins; the consults list grows by one. */
export async function reaskIntervention(
  core: CoreClient,
  chatId: string,
  id: string,
  overrides: InterventionOverrides,
): Promise<{ preview: InterventionPreview; consults: InterventionHoldView['consults'] }> {
  const data = await core.dispatchData({ type: 'chatCustomToolInterventionReask', chatId, id, overrides });
  return data as unknown as { preview: InterventionPreview; consults: InterventionHoldView['consults'] };
}

/** Answer a hold: deal it (perhaps changed), let it stand as rolled, or set the table aside. */
export async function resolveIntervention(
  core: CoreClient,
  chatId: string,
  id: string,
  decision: InterventionDecision,
): Promise<{ resolved: InterventionDecision['kind'] }> {
  const data = await core.dispatchData({ type: 'chatCustomToolInterventionResolve', chatId, id, decision });
  return data as unknown as { resolved: InterventionDecision['kind'] };
}
