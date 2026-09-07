import { ComponentFixture, TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { WizardDescriptionSourceStep } from './description-source-step';
import { WizardState } from './wizard-state';

class FakeCore {
  readonly events$ = new Subject<Record<string, unknown>>().asObservable();
  async dispatchData(): Promise<Record<string, unknown>> {
    return {};
  }
}

async function render(): Promise<{
  fixture: ComponentFixture<WizardDescriptionSourceStep>;
  wizard: WizardState;
}> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WizardDescriptionSourceStep],
    providers: [
      provideTanStackQuery(new QueryClient()),
      WizardState,
      { provide: CoreClient, useValue: new FakeCore() },
    ],
  });
  const wizard = TestBed.inject(WizardState);
  wizard.configure({
    characterId: signal<string | undefined>(undefined),
    characterName: signal(''),
    currentData: signal({}),
    onApply: () => {},
    onClose: () => {},
  });
  const fixture = TestBed.createComponent(WizardDescriptionSourceStep);
  fixture.detectChanges();
  return { fixture, wizard };
}

/** v4 `DescriptionSourceStep.tsx:397-435` — the needs-vision hint. */
describe('WizardDescriptionSourceStep — the needs-vision hint (Tier 2)', () => {
  it('is absent when the source is "existing" even with no vision-capable primary profile', async () => {
    const { fixture, wizard } = await render();
    wizard.profiles.set([{ id: 'p1', name: 'Text', provider: 'OPENAI', modelName: 'gpt', isDefault: true } as never]);
    wizard.primaryProfileId.set('p1');
    wizard.descriptionSource.set('existing');
    fixture.detectChanges();

    expect((fixture.nativeElement as HTMLElement).textContent).not.toContain('Vision Profile Required');
  });

  it('appears for upload/gallery when the primary profile lacks vision', async () => {
    const { fixture, wizard } = await render();
    wizard.profiles.set([{ id: 'p1', name: 'Text', provider: 'OPENAI', modelName: 'gpt', isDefault: true } as never]);
    wizard.primaryProfileId.set('p1');
    wizard.descriptionSource.set('upload');
    fixture.detectChanges();

    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Vision Profile Required');
  });

  it('does not appear when the primary profile already supports vision', async () => {
    const { fixture, wizard } = await render();
    wizard.profiles.set([
      { id: 'p1', name: 'Vision', provider: 'ANTHROPIC', modelName: 'claude', supportsImageUpload: true } as never,
    ]);
    wizard.primaryProfileId.set('p1');
    wizard.descriptionSource.set('gallery');
    fixture.detectChanges();

    expect((fixture.nativeElement as HTMLElement).textContent).not.toContain('Vision Profile Required');
  });

  it("names the fallback profiles when none support vision (v4's exact sentence)", async () => {
    const { fixture, wizard } = await render();
    wizard.profiles.set([{ id: 'p1', name: 'Text', provider: 'OPENAI', modelName: 'gpt', isDefault: true } as never]);
    wizard.primaryProfileId.set('p1');
    wizard.descriptionSource.set('upload');
    fixture.detectChanges();

    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'No vision-capable profiles available. Please create a profile with OpenAI, Anthropic, Google, or Grok.',
    );
  });
});
