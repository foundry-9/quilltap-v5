import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { Subject } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import frameFixture from './fixtures/wizard-progress-frames.json';
import { WizardState } from './wizard-state';

interface RecordedRequest {
  type: string;
  [key: string]: unknown;
}

/**
 * A minimal `CoreClient` stand-in: `dispatchData('characterWizardStream')`
 * replays `frameFixture.frames` — every frame but the last over `events$`
 * (the §B.1 intermediate-frame path), the last (`done`) as the resolved
 * `{ terminal }` body (the §B.1 dispatch-resolution path) — so the parity
 * spec below exercises BOTH delivery mechanisms the real wire uses.
 */
class FakeWizardCore {
  readonly requests: RecordedRequest[] = [];
  private readonly frames = new Subject<Record<string, unknown>>();
  readonly events$ = this.frames.asObservable();

  async dispatchData(request: RecordedRequest): Promise<Record<string, unknown>> {
    this.requests.push(request);
    if (request['type'] === 'connectionProfileList') {
      return {
        profiles: [{ id: 'p1', name: 'Test Profile', provider: 'OPENAI', modelName: 'gpt-test', isDefault: true }],
      };
    }
    if (request['type'] === 'characterWizardStream') {
      const progressId = request['progressId'] as string;
      const all = frameFixture.frames;
      for (const frame of all.slice(0, -1)) {
        this.frames.next({
          type: 'generatorProgress',
          progressId,
          generator: 'wizard',
          event: frame,
        });
      }
      const terminal = all[all.length - 1];
      return { terminal };
    }
    throw new Error(`FakeWizardCore: unexpected request ${request['type']}`);
  }
}

function setUp() {
  const fake = new FakeWizardCore();
  TestBed.configureTestingModule({
    providers: [WizardState, { provide: CoreClient, useValue: fake }],
  });
  const wizard = TestBed.inject(WizardState);
  wizard.configure({
    characterId: signal<string | undefined>('char-1'),
    characterName: signal('Aria'),
    currentData: signal({}),
    onApply: () => {},
    onClose: () => {},
  });
  return { wizard, fake };
}

describe('WizardState — the SSE fold (v4 `useAIWizard.ts:327-369`)', () => {
  it('folds field_start/field_complete/field_error over events$, then the terminal done', async () => {
    const { wizard } = setUp();
    wizard.selectedFields.set(new Set(['identity', 'description', 'scenarios']));
    wizard.primaryProfileId.set('p1');

    await wizard.startGeneration();

    expect(wizard.generating()).toBe(false);
    expect(wizard.generationProgress().completedFields).toEqual(['identity', 'scenarios']);
    expect(wizard.generationProgress().snippets['identity']).toBe(
      'A blacksmith who studies forbidden magic.',
    );
    expect(wizard.generationProgress().errors['description']).toBe('The model refused to continue.');
    expect(wizard.generatedData()).toEqual({
      identity: 'A blacksmith who studies forbidden magic.',
      scenarios: [{ title: 'The Forge at Midnight', content: 'Sparks fly in the dark.' }],
    });
  });

  it('mints a fresh progressId per generation and threads it through the request', async () => {
    const { wizard, fake } = setUp();
    wizard.selectedFields.set(new Set(['identity']));
    wizard.primaryProfileId.set('p1');

    await wizard.startGeneration();

    const streamReq = fake.requests.find((r) => r.type === 'characterWizardStream');
    expect(typeof streamReq?.['progressId']).toBe('string');
    expect((streamReq?.['progressId'] as string).length).toBeGreaterThan(0);
  });

  it('refuses with no dispatch when no field is selected (v4 `startGeneration:249-252`)', async () => {
    const { wizard, fake } = setUp();
    wizard.primaryProfileId.set('p1');

    await wizard.startGeneration();

    expect(wizard.error()).toBe('Please select at least one field to generate');
    expect(fake.requests.some((r) => r.type === 'characterWizardStream')).toBe(false);
  });
});
