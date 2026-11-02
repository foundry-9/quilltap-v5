import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { ModelSelector } from './model-selector';

/**
 * The model combobox's accessible name (P4.D358, v4 `eed72d60c`
 * `components/settings/model-selector.tsx`): an optional `id` threads to the
 * control so a caller's `<label for>` names it; without one the placeholder
 * is the name (`ariaLabel = id ? undefined : placeholder`); the search mode's
 * chevron is `Toggle model list` with `aria-expanded`.
 */

const FEW = ['alpha', 'beta'];
const MANY = Array.from({ length: 12 }, (_, i) => `model-${i}`);

@Component({
  imports: [ModelSelector],
  template: `
    <qt-model-selector [id]="id()" [models]="models()" placeholder="Pick a model" />
  `,
})
class Host {
  readonly id = signal<string | undefined>(undefined);
  readonly models = signal<string[]>(FEW);
}

async function render(id: string | undefined, models: string[]): Promise<ComponentFixture<Host>> {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.id.set(id);
  fixture.componentInstance.models.set(models);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

describe('ModelSelector accessible name', () => {
  it('threads `id` to the select and the search input (and drops the placeholder name)', async () => {
    const few = await render('qt-model-x', FEW);
    const select = few.nativeElement.querySelector('select') as HTMLSelectElement;
    expect(select.id).toBe('qt-model-x');
    expect(select.hasAttribute('aria-label')).toBe(false);
    TestBed.resetTestingModule();

    const many = await render('qt-model-y', MANY);
    const input = many.nativeElement.querySelector('input') as HTMLInputElement;
    expect(input.id).toBe('qt-model-y');
    expect(input.hasAttribute('aria-label')).toBe(false);
  });

  it('falls back to the placeholder as the aria-label when no id is given', async () => {
    const few = await render(undefined, FEW);
    const select = few.nativeElement.querySelector('select') as HTMLSelectElement;
    expect(select.hasAttribute('id')).toBe(false);
    expect(select.getAttribute('aria-label')).toBe('Pick a model');
    TestBed.resetTestingModule();

    const many = await render(undefined, MANY);
    const input = many.nativeElement.querySelector('input') as HTMLInputElement;
    expect(input.getAttribute('aria-label')).toBe('Pick a model');
  });

  it('names the chevron `Toggle model list` and flips its aria-expanded', async () => {
    const fixture = await render(undefined, MANY);
    const chevron = fixture.nativeElement.querySelector(
      'button[aria-label="Toggle model list"]',
    ) as HTMLButtonElement;
    expect(chevron).not.toBeNull();
    expect(chevron.getAttribute('aria-expanded')).toBe('false');

    chevron.click();
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(chevron.getAttribute('aria-expanded')).toBe('true');

    chevron.click();
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(chevron.getAttribute('aria-expanded')).toBe('false');
  });
});
