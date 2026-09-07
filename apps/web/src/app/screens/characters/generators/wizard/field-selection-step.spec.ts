import { ComponentFixture, TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { Subject } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { WizardFieldSelectionStep } from './field-selection-step';
import { WizardState } from './wizard-state';

class FakeCore {
  readonly events$ = new Subject<Record<string, unknown>>().asObservable();
  async dispatchData(): Promise<Record<string, unknown>> {
    return {};
  }
}

async function render(): Promise<{ fixture: ComponentFixture<WizardFieldSelectionStep>; wizard: WizardState }> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WizardFieldSelectionStep],
    providers: [WizardState, { provide: CoreClient, useValue: new FakeCore() }],
  });
  const wizard = TestBed.inject(WizardState);
  wizard.configure({
    characterId: signal<string | undefined>(undefined),
    characterName: signal('Aria'),
    currentData: signal({ identity: 'Already has one.' }),
    onApply: () => {},
    onClose: () => {},
  });
  const fixture = TestBed.createComponent(WizardFieldSelectionStep);
  fixture.detectChanges();
  return { fixture, wizard };
}

/** v4 `FieldSelectionStep.tsx:173-176` — the empty-selection guard copy. */
describe('WizardFieldSelectionStep — the empty-selection guard (Tier 2)', () => {
  it('shows "No fields selected..." when nothing is checked', async () => {
    const { fixture } = await render();
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'No fields selected. Please select at least one field to generate.',
    );
  });

  it('switches to the count summary once a field is selected', async () => {
    const { fixture, wizard } = await render();
    wizard.selectedFields.set(new Set(['scenarios']));
    fixture.detectChanges();

    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).not.toContain('No fields selected.');
    expect(text).toContain('Will generate');
    expect(text).toContain('1');
  });

  it('disables an unavailable field and shows "(has content)" for a filled one', async () => {
    const { fixture } = await render();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    // `characterName` is non-empty ⇒ 'name' is unavailable; 'identity' has content.
    expect(text).toContain('(has content)');
    const nameCheckbox = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('input[type=checkbox]'),
    ).find((el) => el.closest('label')?.textContent?.includes('Name')) as HTMLInputElement;
    expect(nameCheckbox.disabled).toBe(true);
  });
});
