import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Subject } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CoreRequest, ScopedEvent } from '../../../../core/core-contract';
import { CharacterOptimizerModal } from './character-optimizer-modal';
import { OptimizerState } from './optimizer-state';

/**
 * P4.84 — the two optimizer divergences the `p4.9k` round recorded by phrase
 * only, both located against v4 `components/characters/optimizer/` at
 * `2f4254b42` and dispositioned here.
 */

function stubCore(route: (req: CoreRequest) => Record<string, unknown> | Error): CoreClient {
  return {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const out = route(req);
      if (out instanceof Error) throw out;
      return out;
    }),
    events$: new Subject<ScopedEvent>().asObservable(),
  } as unknown as CoreClient;
}

function makeState(core: CoreClient): OptimizerState {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [OptimizerState, { provide: CoreClient, useValue: core }],
  });
  return TestBed.inject(OptimizerState);
}

async function startWith(terminal: unknown): Promise<OptimizerState> {
  const state = makeState(
    stubCore((req) =>
      req.type === 'characterOptimize'
        ? ({ terminal } as Record<string, unknown>)
        : new Error(`unexpected ${req.type}`),
    ),
  );
  await state.startOptimization('c1', 'p1', {} as never, 'apply');
  return state;
}

// ---------------------------------------------------------------------------
// Divergence 2 — the error-terminal phase / a stream that ends without `done`.
// ---------------------------------------------------------------------------

describe('OptimizerState — v4’s terminal arms (useCharacterOptimizer.ts:220-238)', () => {
  it('an `error` terminal stops the spinner and leaves the PHASE where it was', async () => {
    // v4 `:220-226`: `setError(...)` + `setLoading(false)`, no `setPhase`.
    // `startOptimization` sets the phase to `progress` at the top, so that is
    // where a failed run stays — which is where the modal renders the message.
    const state = await startWith({ type: 'error', error: 'the automata are indisposed' });
    expect(state.error()).toBe('the automata are indisposed');
    expect(state.loading()).toBe(false);
    expect(state.phase()).toBe('progress');
  });

  it('an `error` terminal with no message takes v4’s fixed sentence', async () => {
    const state = await startWith({ type: 'error' });
    expect(state.error()).toBe('An unforeseen calamity has befallen the refinement process.');
  });

  it('a resolution with NO terminal stops the spinner (v4 :233-238)', async () => {
    // v4's read loop can end without ever seeing a `done` frame; it then calls
    // `setLoading(false)` and, if any suggestions landed, `setPhase('review')`.
    // v5's equivalent is a dispatch that resolves carrying no terminal event —
    // before this, `applyOptimizerEvent`'s `default` arm returned the state
    // unchanged and the modal span forever.
    const state = await startWith(undefined);
    expect(state.loading()).toBe(false);
    // No suggestions arrived, so v4 leaves the phase alone.
    expect(state.phase()).toBe('progress');
    expect(state.error()).toBeNull();
  });

  it('a resolution with no terminal shows the suggestions that DID land, in review', async () => {
    // Suggestions must arrive the way they really do — through the progress
    // stream, DURING the dispatch — because `startOptimization` resets the fold
    // at the top, so anything seeded before the call is wiped (the trap that
    // made an earlier draft of this case measure the wrong state).
    const events = new Subject<ScopedEvent>();
    const core = {
      dispatchData: vi.fn(async (req: CoreRequest) => {
        if (req.type !== 'characterOptimize') throw new Error(`unexpected ${req.type}`);
        events.next({
          type: 'generatorProgress',
          progressId: (req as unknown as { progressId: string }).progressId,
          generator: 'optimizer',
          event: {
            type: 'substep_complete',
            partialSuggestions: [
              {
                id: 's1',
                field: 'identity',
                currentValue: 'old',
                proposedValue: 'new',
                rationale: 'r',
                significance: 'moderate',
              },
            ],
          },
        } as unknown as ScopedEvent);
        // …and then the run ends carrying no terminal at all.
        return { terminal: { type: 'not_a_terminal' } } as Record<string, unknown>;
      }),
      events$: events.asObservable(),
    } as unknown as CoreClient;

    const state = makeState(core);
    await state.startOptimization('c1', 'p1', {} as never, 'apply');

    expect(state.suggestions()).toHaveLength(1);
    expect(state.loading()).toBe(false);
    expect(state.phase()).toBe('review');
  });

  it('a `done` terminal is untouched by the stream-end arm', async () => {
    const state = await startWith({ type: 'done', suggestions: [] });
    expect(state.loading()).toBe(false);
    // The no-suggestions branch, not `review` — the stream-end arm must not
    // reach a state a terminal already settled.
    expect(state.phase()).toBe('progress');
    expect(state.noSuggestionsMessage()).toContain('already quite splendidly rendered');
  });
});

// ---------------------------------------------------------------------------
// Divergence 1 — the stale-closure apply banner. A v4 BUG; v5 is correct.
// ---------------------------------------------------------------------------

@Component({
  imports: [CharacterOptimizerModal],
  template: `<qt-character-optimizer-modal
    [characterId]="'c1'"
    [characterName]="'Perpetua'"
    [profiles]="[]"
    [defaultConnectionProfileId]="null"
    [vaultAvailable]="false"
  />`,
})
class ModalHost {}

describe('the optimizer’s apply banner — v4 reads a STALE closure (v4-first filing candidate)', () => {
  /**
   * v4 `CharacterOptimizerModal.tsx:105-113`:
   *
   * ```
   * const handleApply = async () => {
   *   await optimizer.applyChanges(characterId)
   *   if (!optimizer.error) { setApplySuccess(true); setTimeout(onApplied, 1500) }
   * }
   * ```
   *
   * `optimizer.error` is the value captured by the render that created this
   * handler. `applyChanges` sets it through `setError`, which cannot mutate that
   * captured value — so after a FAILED apply v4 reads the pre-call `null`, shows
   * "Refinements Commissioned", closes the modal 1.5 s later, and its own error
   * pane is suppressed by the `!applySuccess` guard at `:495`. A failed apply
   * reports success and loses the message.
   *
   * v5 reads `state.error()`, a signal, so it sees the current value. That is a
   * DIVERGENCE, deliberately kept: the v4 behaviour is a bug, not a contract.
   * Recorded as a v4-first filing candidate with the repro below.
   */
  it('a failed apply shows the error, not the success banner', async () => {
    const core = stubCore((req) => {
      if (req.type === 'characterUpdate') return new Error('the dossier is locked');
      if (req.type === 'characterGet') return { character: { id: 'c1' } };
      return {};
    });
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [ModalHost],
      providers: [{ provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(ModalHost);
    fixture.detectChanges();

    const modal = fixture.debugElement.children[0].componentInstance as CharacterOptimizerModal;
    const state = TestBed.inject(OptimizerState, undefined, { optional: true });
    // The modal provides its own OptimizerState instance; reach it through the
    // component, which is how the template reads it too.
    const inner = (modal as unknown as { state: OptimizerState }).state;
    expect(state === null || state !== inner).toBe(true);

    // One accepted suggestion, so `applyChanges` actually runs.
    (
      inner as unknown as { fold: { update(f: (s: never) => unknown): void } }
    ).fold.update((s) => ({
      ...(s as object),
      phase: 'apply',
      suggestions: [
        {
          id: 's1',
          field: 'identity',
          currentValue: 'old',
          proposedValue: 'new',
          rationale: 'r',
          significance: 'moderate',
        },
      ],
    }));
    inner.decideSuggestion('s1', 'accepted');
    fixture.detectChanges();

    await (modal as unknown as { handleApply(): Promise<void> }).handleApply();
    fixture.detectChanges();

    // The message survived …
    expect(inner.error()).toBeTruthy();
    // … and the success banner did NOT go up. v4's stale read shows it here.
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).not.toContain('Refinements Commissioned');
    expect(text).toContain(inner.error() as string);
  });
});
