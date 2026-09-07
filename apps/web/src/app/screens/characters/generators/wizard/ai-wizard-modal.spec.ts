import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import frameFixture from './fixtures/wizard-progress-frames.json';
import { AiWizardModal } from './ai-wizard-modal';
import type { GeneratedCharacterData } from '../edit-generators.api';

type Req = { type: string; [k: string]: unknown };

class FakeCore {
  readonly requests: Req[] = [];
  private readonly frames = new Subject<Record<string, unknown>>();
  readonly events$ = this.frames.asObservable();

  async dispatchData(req: Req): Promise<Record<string, unknown>> {
    this.requests.push(req);
    if (req.type === 'connectionProfileList') {
      return {
        profiles: [
          { id: 'p1', name: 'Text Model', provider: 'OPENAI', modelName: 'gpt-4', isDefault: true },
          { id: 'p2', name: 'Vision Model', provider: 'ANTHROPIC', modelName: 'claude', supportsImageUpload: true },
        ],
      };
    }
    if (req.type === 'characterWizardStream') {
      const progressId = req['progressId'] as string;
      const all = frameFixture.frames;
      for (const frame of all.slice(0, -1)) {
        this.frames.next({ type: 'generatorProgress', progressId, generator: 'wizard', event: frame });
      }
      return { terminal: all[all.length - 1] };
    }
    return {};
  }
}

async function render(): Promise<{ fixture: ComponentFixture<AiWizardModal>; core: FakeCore }> {
  const core = new FakeCore();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [AiWizardModal],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: core },
    ],
  });
  const fixture = TestBed.createComponent(AiWizardModal);
  fixture.componentRef.setInput('characterName', '');
  fixture.componentRef.setInput('currentData', {});
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, core };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function clickButtonWithText(fixture: ComponentFixture<unknown>, text: string): void {
  const button = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === text,
  ) as HTMLButtonElement | undefined;
  if (!button) throw new Error(`no button with text "${text}"`);
  button.click();
}

/**
 * The four-step wizard flow, end to end, over a fake `CoreClient` — the
 * jsdom-level twin of the gated e2e beat (`character-wizard-flow.spec.ts`),
 * runnable without a real server.
 */
describe('AiWizardModal — the four-step flow (v4 `AIWizardModal.tsx`)', () => {
  it('walks Select Model → Description Source → Fields → Generate → Apply', async () => {
    const { fixture } = await render();
    const text = () => (fixture.nativeElement as HTMLElement).textContent ?? '';

    expect(text()).toContain('Select AI Model');
    // Default profile auto-selected (v4 `:78-83`) — Next is enabled.
    clickButtonWithText(fixture, 'Next');
    await settle(fixture);

    expect(text()).toContain('Physical Description Source');
    // 'existing' is the default source — proceed straight through.
    clickButtonWithText(fixture, 'Next');
    await settle(fixture);

    expect(text()).toContain('Select Fields');
    // Select one field.
    const identityCheckbox = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('input[type=checkbox]'),
    ).find((el) => el.closest('label')?.textContent?.includes('Identity')) as HTMLInputElement;
    identityCheckbox.click();
    await settle(fixture);

    clickButtonWithText(fixture, 'Review & Generate');
    await settle(fixture);

    expect(text()).toContain('Ready to Generate');
    clickButtonWithText(fixture, 'Generate Character Content');
    await settle(fixture);

    expect(text()).toContain('Generation Complete');

    const applyEvents: GeneratedCharacterData[] = [];
    fixture.componentInstance.apply.subscribe((d: GeneratedCharacterData) => applyEvents.push(d));
    let closeCount = 0;
    fixture.componentInstance.closeModal.subscribe(() => { closeCount++; });

    clickButtonWithText(fixture, 'Apply to Character');
    await settle(fixture);

    expect(applyEvents).toHaveLength(1);
    expect(applyEvents[0].identity).toBe('A blacksmith who studies forbidden magic.');
    expect(closeCount).toBe(1);
  });

  it('Back is disabled on step 1 and Next is disabled without a selected field on step 3', async () => {
    const { fixture } = await render();
    const backButton = () =>
      Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
        (b) => b.textContent?.trim() === 'Back',
      ) as HTMLButtonElement;

    expect(backButton().disabled).toBe(true);

    clickButtonWithText(fixture, 'Next');
    await settle(fixture);
    clickButtonWithText(fixture, 'Next');
    await settle(fixture);

    const reviewButton = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Review & Generate') as HTMLButtonElement;
    expect(reviewButton.disabled).toBe(true);
  });

  it('the close button is disabled while generating (v4 `:84`)', async () => {
    const { fixture } = await render();
    fixture.componentInstance['wizard'].currentStep.set(4);
    fixture.componentInstance['wizard'].generating.set(true);
    fixture.detectChanges();

    const closeButton = (fixture.nativeElement as HTMLElement).querySelector(
      '.qt-dialog-header button',
    ) as HTMLButtonElement;
    expect(closeButton.disabled).toBe(true);
  });
});
