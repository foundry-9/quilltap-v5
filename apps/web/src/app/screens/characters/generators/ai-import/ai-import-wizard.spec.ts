import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Subject } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CoreRequest, ScopedEvent } from '../../../../core/core-contract';
import { applyAiImportEvent, AI_IMPORT_FOLD_INITIAL, type AiImportFoldState } from './ai-import-fold';
import {
  AI_IMPORT_INITIAL_STEPS,
  CORE_STEPS,
  STEP_DISPLAY_NAMES,
  type AIImportStepName,
} from './ai-import.types';
import { AiImportWizard } from './ai-import-wizard';
import type { AiImportState } from './ai-import-state';

type AnyRequest = CoreRequest & Record<string, unknown>;

// ===========================================================================
// §1 — parity: `ai-import.types.ts`'s transcription of v4
// `components/settings/ai-import/types.ts` against a SECOND, independently
// written transcription here, so the two copies cannot drift into agreement
// by accident (the nanogpt-options.spec.ts precedent).
// ===========================================================================

/** v4 `STEP_DISPLAY_NAMES` (`types.ts:78-91`), transcribed independently. */
const EXPECTED_STEP_DISPLAY_NAMES: Record<AIImportStepName, string> = {
  analyzing: 'Analyzing Source Material',
  character_basics: 'Extracting Character Basics',
  first_message: 'Generating Dialogue',
  system_prompts: 'Creating System Prompts',
  physical_descriptions: 'Describing Appearance',
  wardrobe_items: 'Generating Wardrobe',
  pronouns: 'Determining Pronouns & Aliases',
  memories: 'Generating Memories',
  chats: 'Creating Example Chat',
  assembly: 'Assembling Export',
  validation: 'Validating Data',
  repair: 'Repairing Issues',
};

/** v4 `CORE_STEPS` (`types.ts:94-103`), transcribed independently. */
const EXPECTED_CORE_STEPS: AIImportStepName[] = [
  'character_basics',
  'first_message',
  'system_prompts',
  'physical_descriptions',
  'wardrobe_items',
  'pronouns',
  'assembly',
  'validation',
];

describe('ai-import.types — parity with v4 components/settings/ai-import/types.ts', () => {
  it('STEP_DISPLAY_NAMES matches an independent transcription', () => {
    expect(STEP_DISPLAY_NAMES).toEqual(EXPECTED_STEP_DISPLAY_NAMES);
  });

  it('CORE_STEPS matches an independent transcription', () => {
    expect(CORE_STEPS).toEqual(EXPECTED_CORE_STEPS);
  });

  it('AI_IMPORT_INITIAL_STEPS seeds every step name pending, and its key set matches STEP_DISPLAY_NAMES', () => {
    expect(Object.keys(AI_IMPORT_INITIAL_STEPS).sort()).toEqual(
      Object.keys(EXPECTED_STEP_DISPLAY_NAMES).sort(),
    );
    for (const status of Object.values(AI_IMPORT_INITIAL_STEPS)) {
      expect(status).toEqual({ status: 'pending' });
    }
  });
});

// ===========================================================================
// §2 — the pure reducer, driven through a recorded v4-shaped SSE sequence
// (useAIImport.ts's `switch (event.type)`, `:248-305`).
// ===========================================================================

describe('applyAiImportEvent', () => {
  it('folds a step_start / step_complete / step_error / done sequence exactly like v4', () => {
    let state: AiImportFoldState = {
      ...AI_IMPORT_FOLD_INITIAL,
      generating: true,
      steps: { ...AI_IMPORT_INITIAL_STEPS },
    };

    // step_start: character_basics begins.
    state = applyAiImportEvent(state, { type: 'step_start', step: 'character_basics' });
    expect(state.steps.character_basics).toEqual({ status: 'in_progress' });
    // Untouched steps stay pending.
    expect(state.steps.first_message).toEqual({ status: 'pending' });

    // step_complete: character_basics finishes with a snippet.
    state = applyAiImportEvent(state, {
      type: 'step_complete',
      step: 'character_basics',
      snippet: 'Ariadne, a research librarian…',
    });
    expect(state.steps.character_basics).toEqual({
      status: 'complete',
      snippet: 'Ariadne, a research librarian…',
    });

    // step_start: wardrobe_items begins, then fails.
    state = applyAiImportEvent(state, { type: 'step_start', step: 'wardrobe_items' });
    state = applyAiImportEvent(state, {
      type: 'step_error',
      step: 'wardrobe_items',
      error: 'The provider declined to answer.',
    });
    expect(state.steps.wardrobe_items).toEqual({
      status: 'error',
      error: 'The provider declined to answer.',
    });
    expect(state.errors).toEqual({ wardrobe_items: 'The provider declined to answer.' });

    // done: the terminal event, applied once (§B.1 — never from the stream).
    state = applyAiImportEvent(state, {
      type: 'done',
      result: { manifest: { format: 'quilltap-export' } },
      stepResults: { character_basics: { name: 'Ariadne' } },
      errors: { wardrobe_items: 'The provider declined to answer.' },
    });
    expect(state.generating).toBe(false);
    expect(state.result).toEqual({ manifest: { format: 'quilltap-export' } });
    expect(state.stepResults).toEqual({ character_basics: { name: 'Ariadne' } });
    expect(state.errors).toEqual({ wardrobe_items: 'The provider declined to answer.' });
  });

  it('a step_error with no `error` field falls back to v4\'s "Step failed" (:284)', () => {
    const state = applyAiImportEvent(AI_IMPORT_FOLD_INITIAL, { type: 'step_error', step: 'pronouns' });
    expect(state.errors).toEqual({ pronouns: 'Step failed' });
    expect(state.steps.pronouns).toEqual({ status: 'error', error: undefined });
  });

  it('an unrecognized event type leaves state unchanged (the `default` branch)', () => {
    const before: AiImportFoldState = {
      ...AI_IMPORT_FOLD_INITIAL,
      generating: true,
      steps: { ...AI_IMPORT_INITIAL_STEPS, analyzing: { status: 'in_progress' } },
    };
    const after = applyAiImportEvent(before, { type: 'heartbeat' });
    expect(after).toBe(before);
  });

  it('a `done` event that carries a top-level `error` still surfaces it (v4 :301-303)', () => {
    const state = applyAiImportEvent(
      { ...AI_IMPORT_FOLD_INITIAL, generating: true },
      { type: 'done', error: 'The connection profile could not be reached.' },
    );
    expect(state.generating).toBe(false);
    expect(state.error).toBe('The connection profile could not be reached.');
    // No `result`/`stepResults` on this frame — the prior (null) values persist.
    expect(state.result).toBeNull();
    expect(state.stepResults).toBeNull();
  });

  it('a `done` event that omits `errors` keeps whatever per-step errors already accumulated', () => {
    const withErrors: AiImportFoldState = {
      ...AI_IMPORT_FOLD_INITIAL,
      errors: { memories: 'Ran out of budget.' },
    };
    const state = applyAiImportEvent(withErrors, { type: 'done', result: { ok: true } });
    expect(state.errors).toEqual({ memories: 'Ran out of budget.' });
  });
});

// ===========================================================================
// §3 — component smoke render (TestBed). The mutation-proven logic lives in
// §2's reducer tests; this only confirms the wizard mounts and reads state
// through the injected `AiImportState` without throwing.
// ===========================================================================

function stubCore(route: (req: AnyRequest) => Record<string, unknown> | Error): CoreClient {
  return {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const out = route(req as AnyRequest);
      if (out instanceof Error) throw out;
      return out;
    }),
    events$: new Subject<ScopedEvent>().asObservable(),
  } as unknown as CoreClient;
}

/** Reach the protected `state` field for direct navigation in the smoke test. */
function innerState(component: AiImportWizard): { state: AiImportState } {
  return component as unknown as { state: AiImportState };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

describe('AiImportWizard — smoke render', () => {
  it('mounts on Step 1 (Source Material) and lists the fetched connection profiles', async () => {
    const core = stubCore((req) => {
      if (req.type === 'connectionProfileList') {
        return {
          profiles: [
            { id: 'p1', name: 'Household GPT', provider: 'openai', modelName: 'gpt-5' },
            { id: 'p2', name: 'The Understudy', provider: 'nanogpt', modelName: 'chroma' },
          ],
        };
      }
      return new Error(`unexpected ${req.type}`);
    });

    TestBed.configureTestingModule({
      imports: [AiImportWizard],
      providers: [{ provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(AiImportWizard);
    fixture.detectChanges();
    await settle(fixture);

    const root: HTMLElement = fixture.nativeElement;
    expect(root.textContent).toContain('Source Material');
    expect(root.textContent).toContain('Drop files here or click to browse');
    expect(root.textContent).toContain('Paste any additional character information');

    // Advance to Configuration and check the fetched profiles rendered.
    innerState(fixture.componentInstance).state.nextStep();
    fixture.detectChanges();
    await settle(fixture);
    expect(root.textContent).toContain('Household GPT');
    expect(root.textContent).toContain('The Understudy');
  });

  it('emits `closed` when the header close button is pressed', async () => {
    const core = stubCore((req) =>
      req.type === 'connectionProfileList' ? { profiles: [] } : new Error(`unexpected ${req.type}`),
    );

    TestBed.configureTestingModule({
      imports: [AiImportWizard],
      providers: [{ provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(AiImportWizard);
    fixture.detectChanges();
    await settle(fixture);

    let closed = false;
    fixture.componentInstance.closed.subscribe(() => {
      closed = true;
    });

    const closeButton = fixture.nativeElement.querySelector('button[aria-label="Close"]') as HTMLButtonElement;
    closeButton.click();
    expect(closed).toBe(true);
  });
});

// ===========================================================================
// §4 — P4.84: the fixed chrome title, and v4's dead `(N entities imported)`
// line. Both measured against v4 at `2f4254b42`.
// ===========================================================================

describe('AiImportWizard — the chrome title (v4 SummonFromLoreModal / AuroraView)', () => {
  async function mount(): Promise<ComponentFixture<AiImportWizard>> {
    const core = stubCore((req) =>
      req.type === 'connectionProfileList' ? { profiles: [] } : new Error(`unexpected ${req.type}`),
    );
    TestBed.configureTestingModule({
      imports: [AiImportWizard],
      providers: [{ provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(AiImportWizard);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('titles the dialog "Summon From Lore", not the current step', async () => {
    // v4 never mounts `AIImportWizard` bare: both call sites wrap it in chrome
    // carrying this fixed title (`SummonFromLoreModal.tsx:70-73` on the Salon
    // side, `AuroraView.tsx:683-691` on the roster side). v5 folded the wrapper
    // into the dialog, so the dialog must carry the title itself.
    const fixture = await mount();
    const title = fixture.nativeElement.querySelector('.qt-dialog-title') as HTMLElement;
    expect(title.textContent?.trim()).toBe('Summon From Lore');
  });

  it('keeps the step name as a section heading under the indicator (v4 :705-707)', async () => {
    const fixture = await mount();
    const heading = fixture.nativeElement.querySelector('.qt-section-title') as HTMLElement;
    expect(heading.textContent?.trim()).toBe('Source Material');

    // And it tracks the step, where the dialog title does not.
    innerState(fixture.componentInstance).state.nextStep();
    fixture.detectChanges();
    await settle(fixture);
    expect(
      (fixture.nativeElement.querySelector('.qt-section-title') as HTMLElement).textContent?.trim(),
    ).toBe('Configuration');
    expect(
      (fixture.nativeElement.querySelector('.qt-dialog-title') as HTMLElement).textContent?.trim(),
    ).toBe('Summon From Lore');
  });
});

describe("AiImportWizard — v4's `(N entities imported)` line is dead code", () => {
  /**
   * MEASURED at `2f4254b42`, not inferred. `import-execute` answers
   * `ImportResult`, whose `imported` is a `QuilltapExportCounts` OBJECT
   * (`lib/import/quilltap-import/types.ts:169-181`); `useAIImport.ts:351` stores
   * it as `importedCount` typed `number`; the render gate at
   * `AIImportWizard.tsx:480` then compares that object with `>`, which coerces
   * to `NaN`, which is never `> 1`. The line cannot render in v4 on ANY import.
   *
   * v5 used to sum the counts and show it. That is the divergence this pins
   * shut: with a genuine multi-entity result the line stays absent, exactly as
   * v4's does. It is a v4-first filing candidate, not a v5 behaviour to keep.
   */
  it('stays silent on a multi-entity import, as v4 does', async () => {
    const core = stubCore((req) =>
      req.type === 'connectionProfileList' ? { profiles: [] } : new Error(`unexpected ${req.type}`),
    );
    TestBed.configureTestingModule({
      imports: [AiImportWizard],
      providers: [{ provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(AiImportWizard);
    fixture.detectChanges();
    await settle(fixture);

    // Four entities across three types — the shape v5 used to render as
    // "(4 entities imported)".
    const imported = { characters: 1, tags: 3 };
    const line = (
      fixture.componentInstance as unknown as {
        importedCountLine(): string | null;
      }
    ).importedCountLine.bind(fixture.componentInstance);
    (
      innerState(fixture.componentInstance).state as unknown as {
        importResultSig: { set(v: unknown): void };
      }
    ).importResultSig.set({
      success: true,
      imported,
      warnings: [],
      importedCharacterIds: ['c1'],
    });
    fixture.detectChanges();

    expect(line()).toBeNull();
    // The sum v5 used to show, spelled out so the case cannot go vacuous by
    // the counts happening to be empty.
    expect(Object.values(imported).reduce((a, b) => a + b, 0)).toBe(4);
    // And v4's own arithmetic, checked rather than asserted.
    expect(Number(imported as unknown as number)).toBeNaN();
  });
});
