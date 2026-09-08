import { computed, DestroyRef, inject, Injectable, signal } from '@angular/core';
import type { Subscription } from 'rxjs';

import { CoreClient } from '../../../../core/core-client';
import { fetchCharacter } from '../../characters.api';
import { asGeneratorProgress, dispatchCharacterOptimize, mintProgressId } from '../detail-generators.api';
import type {
  OptimizerFilterOptions,
  OptimizerOutputMode,
  OptimizerPhase,
  OptimizerSuggestion,
  SuggestionDecision,
} from '../detail-generators.api';
import { applyCharacterFieldUpdates } from './apply-character-field-updates';
import {
  buildOptimizerApplyPlan,
  getAcceptedChanges,
  mergeOptimizerApplyPlan,
  type AcceptedChange,
} from './apply-plan-builder';
import { applyOptimizerEvent, OPTIMIZER_FOLD_INITIAL, type OptimizerFoldState } from './optimizer-fold';

/**
 * Modal-scoped state (v4 `useCharacterOptimizer.ts`, transcribed as an
 * Angular injectable rather than a hook — `providers: [OptimizerState]` on
 * `CharacterOptimizerModal` gives every open of the dialog a fresh instance,
 * matching v4's per-mount hook lifetime).
 */
@Injectable()
export class OptimizerState {
  private readonly core = inject(CoreClient);
  private readonly destroyRef = inject(DestroyRef);

  private readonly fold = signal<OptimizerFoldState>(OPTIMIZER_FOLD_INITIAL);
  private readonly decisionsSig = signal<Map<string, SuggestionDecision>>(new Map());
  private readonly editedValuesSig = signal<Map<string, string>>(new Map());
  private readonly currentIndexSig = signal(0);
  private readonly applyingSig = signal(false);
  private readonly startedAtSig = signal<number | null>(null);

  private outputMode: OptimizerOutputMode = 'apply';
  private progressSub: Subscription | null = null;

  constructor() {
    this.destroyRef.onDestroy(() => this.progressSub?.unsubscribe());
  }

  readonly phase = computed<OptimizerPhase>(() => this.fold().phase);
  readonly analysis = computed(() => this.fold().analysis);
  readonly suggestions = computed(() => this.fold().suggestions);
  readonly memoryCount = computed(() => this.fold().memoryCount);
  readonly filteredCount = computed(() => this.fold().filteredCount);
  readonly loading = computed(() => this.fold().loading);
  readonly progressStep = computed(() => this.fold().progressStep);
  readonly progressSubStep = computed(() => this.fold().progressSubStep);
  readonly noSuggestionsMessage = computed(() => this.fold().noSuggestionsMessage);
  readonly suggestionsFilePath = computed(() => this.fold().suggestionsFilePath);
  readonly error = computed(() => this.fold().error);

  readonly currentIndex = this.currentIndexSig.asReadonly();
  readonly decisions = this.decisionsSig.asReadonly();
  readonly editedValues = this.editedValuesSig.asReadonly();
  readonly applying = this.applyingSig.asReadonly();
  readonly startedAt = this.startedAtSig.asReadonly();

  readonly acceptedChanges = computed<AcceptedChange[]>(() =>
    getAcceptedChanges(this.suggestions(), this.decisions(), this.editedValues()),
  );

  setPhase(phase: OptimizerPhase): void {
    this.fold.update((s) => ({ ...s, phase }));
  }

  async startOptimization(
    characterId: string,
    connectionProfileId: string,
    filterOptions: OptimizerFilterOptions,
    outputMode: OptimizerOutputMode,
  ): Promise<void> {
    this.outputMode = outputMode;
    this.fold.set({ ...OPTIMIZER_FOLD_INITIAL, loading: true, phase: 'progress' });
    this.decisionsSig.set(new Map());
    this.editedValuesSig.set(new Map());
    this.currentIndexSig.set(0);
    this.startedAtSig.set(Date.now());

    const progressId = mintProgressId();

    this.progressSub?.unsubscribe();
    this.progressSub = this.core.events$.subscribe((frame) => {
      const gp = asGeneratorProgress(frame, progressId);
      if (!gp || gp.generator !== 'optimizer') return;
      // The terminal (`done`/`error`) is applied once from the dispatch's own
      // resolution below, never from the stream — §B.1 / the almanack-card.ts
      // precedent.
      if (gp.event['type'] === 'done' || gp.event['type'] === 'error') return;
      this.fold.update((s) => applyOptimizerEvent(s, gp.event, this.outputMode));
    });

    try {
      const terminal = await dispatchCharacterOptimize(this.core, {
        type: 'characterOptimize',
        characterId,
        progressId,
        connectionProfileId,
        maxMemories: filterOptions.maxMemories,
        searchQuery: filterOptions.searchQuery,
        useSemanticSearch: filterOptions.useSemanticSearch,
        sinceDate: filterOptions.sinceDate,
        beforeDate: filterOptions.beforeDate,
        outputMode,
      });
      this.fold.update((s) => {
        // `terminal ?? {}`: a resolution that carries no terminal at all is
        // v5's shape of v4's "stream ended without a `done` event", and an
        // undefined event would throw out of the fold instead of taking that
        // arm.
        const next = applyOptimizerEvent(
          s,
          (terminal ?? {}) as unknown as Record<string, unknown>,
          this.outputMode,
        );
        // v4 `useCharacterOptimizer.ts:233-238`: after the read loop ends, if
        // no `done` (or `error`) frame ever arrived, stop the spinner anyway
        // and show whatever suggestions did land. `applyOptimizerEvent` clears
        // `loading` on both terminals, so a still-loading state here means
        // neither was applied — which is exactly v4's condition. The phase is
        // left alone when nothing arrived, as v4 leaves it.
        if (!next.loading) return next;
        return {
          ...next,
          loading: false,
          phase: next.suggestions.length > 0 ? 'review' : next.phase,
        };
      });
    } catch (err) {
      this.fold.update((s) => ({
        ...s,
        loading: false,
        error:
          err instanceof Error && err.message
            ? err.message
            : 'The refinement endeavour encountered an unexpected impediment.',
      }));
    } finally {
      this.progressSub?.unsubscribe();
      this.progressSub = null;
    }
  }

  decideSuggestion(id: string, decision: SuggestionDecision): void {
    this.decisionsSig.update((prev) => new Map(prev).set(id, decision));
  }

  editSuggestion(id: string, newValue: string): void {
    this.editedValuesSig.update((prev) => new Map(prev).set(id, newValue));
    this.decisionsSig.update((prev) => new Map(prev).set(id, 'edited'));
  }

  goToSuggestion(index: number): void {
    if (index >= 0 && index < this.suggestions().length) this.currentIndexSig.set(index);
  }
  nextSuggestion(): void {
    this.currentIndexSig.update((prev) => Math.min(prev + 1, this.suggestions().length - 1));
  }
  prevSuggestion(): void {
    this.currentIndexSig.update((prev) => Math.max(prev - 1, 0));
  }

  /**
   * v4 `applyChanges` (`:303-517`). Fetches the character fresh (so array/
   * object fields merge rather than clobber), fans updates across
   * `applyCharacterFieldUpdates` and the wardrobe verbs, and collects every
   * partial failure into one joined error (v4 never throws mid-fan-out).
   */
  async applyChanges(characterId: string): Promise<void> {
    const accepted = this.acceptedChanges();
    if (accepted.length === 0) return;

    this.applyingSig.set(true);
    this.fold.update((s) => ({ ...s, error: null }));

    try {
      const character = await fetchCharacter(this.core, characterId);

      const plan = buildOptimizerApplyPlan(accepted);
      const mainUpdates = mergeOptimizerApplyPlan(plan, character);

      const { errors } = await applyCharacterFieldUpdates(this.core, characterId, {
        mainUpdates,
        promptUpdates: plan.promptUpdates.map(({ subId, finalValue }) => ({
          id: subId,
          content: finalValue,
        })),
        promptCreates: plan.promptCreates.map(({ name, finalValue }) => ({
          name,
          content: finalValue,
        })),
        messages: {
          promptUpdateFailed: 'A system prompt refinement could not be saved.',
          promptCreateFailed: (name) => `The new system prompt "${name}" could not be saved.`,
          mainPutFailed: 'The amendments could not be inscribed into the character record.',
        },
      });

      const wardrobeErrors: string[] = [];
      for (const { subId, finalValue } of plan.wardrobeUpdates) {
        try {
          await this.core.dispatchData({
            type: 'characterWardrobeUpdate',
            characterId,
            itemId: subId,
            item: { description: finalValue },
          });
        } catch {
          wardrobeErrors.push('A wardrobe item refinement could not be saved.');
        }
      }
      for (const { suggestion, finalValue } of plan.wardrobeCreates) {
        const item = suggestion.wardrobeItem;
        if (!item) continue;
        const description = this.decisions().get(suggestion.id) === 'edited' ? finalValue : item.description;
        try {
          await this.core.dispatchData({
            type: 'characterWardrobeCreate',
            characterId,
            item: {
              title: item.title,
              description: description || null,
              imagePrompt: item.imagePrompt || null,
              types: item.types,
              appropriateness: item.appropriateness || null,
              isDefault: item.isDefault === true,
            },
          });
        } catch {
          wardrobeErrors.push(`The new wardrobe item "${item.title}" could not be saved.`);
        }
      }

      const allErrors = [...errors, ...wardrobeErrors];
      if (allErrors.length > 0) {
        throw new Error(allErrors.join(' '));
      }
    } catch (err) {
      this.fold.update((s) => ({
        ...s,
        error:
          err instanceof Error && err.message
            ? err.message
            : 'The application of refinements met with an unforeseen impediment.',
      }));
    } finally {
      this.applyingSig.set(false);
    }
  }
}

export type { OptimizerSuggestion };
