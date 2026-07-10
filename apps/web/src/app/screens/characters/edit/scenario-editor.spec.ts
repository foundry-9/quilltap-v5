import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { CharacterScenario } from '../../../core/core-contract';
import { ScenarioEditor } from './scenario-editor';

function scenario(over: Partial<CharacterScenario> = {}): CharacterScenario {
  return {
    id: 's1',
    title: 'A rainy evening',
    content: 'The drawing room, past midnight.',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

async function render(scenarios: CharacterScenario[]): Promise<ComponentFixture<ScenarioEditor>> {
  TestBed.configureTestingModule({ imports: [ScenarioEditor] });
  const fixture = TestBed.createComponent(ScenarioEditor);
  fixture.componentRef.setInput('scenarios', scenarios);
  fixture.detectChanges();
  return fixture;
}

describe('ScenarioEditor', () => {
  it('shows the empty state with an "Add First Scenario" affordance', async () => {
    const fixture = await render([]);
    expect(fixture.nativeElement.textContent).toContain('No scenarios yet');
    expect(fixture.nativeElement.textContent).toContain('Add First Scenario');
  });

  it('emits an appended row (client-minted id) on "+ Add Scenario"', async () => {
    const fixture = await render([scenario()]);
    let emitted: CharacterScenario[] | null = null;
    fixture.componentInstance.scenariosChange.subscribe((v) => (emitted = v));

    const addButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === '+ Add Scenario',
    ) as HTMLButtonElement;
    addButton.click();

    expect(emitted).not.toBeNull();
    expect(emitted!.length).toBe(2);
    expect(emitted![0].id).toBe('s1');
    expect(emitted![1].id).toBeTruthy();
    expect(emitted![1].title).toBe('');
    expect(emitted![1].content).toBe('');
  });

  it('emits the array with a row removed on the trash button', async () => {
    const fixture = await render([scenario({ id: 'a' }), scenario({ id: 'b' })]);
    let emitted: CharacterScenario[] | null = null;
    fixture.componentInstance.scenariosChange.subscribe((v) => (emitted = v));

    const trashButtons = fixture.nativeElement.querySelectorAll('[title="Remove scenario"]');
    expect(trashButtons.length).toBe(2);
    (trashButtons[0] as HTMLButtonElement).click();

    expect(emitted).not.toBeNull();
    expect(emitted!.map((s) => s.id)).toEqual(['b']);
  });

  it('updates title and content in place, bumping updatedAt', async () => {
    const fixture = await render([scenario({ id: 'a', updatedAt: '2024-01-01T00:00:00.000Z' })]);
    let emitted: CharacterScenario[] | null = null;
    fixture.componentInstance.scenariosChange.subscribe((v) => (emitted = v));

    const titleInput = fixture.nativeElement.querySelector(
      'input[placeholder="Scenario title"]',
    ) as HTMLInputElement;
    titleInput.value = 'A stormy night';
    titleInput.dispatchEvent(new Event('input'));

    expect(emitted).not.toBeNull();
    expect(emitted![0].title).toBe('A stormy night');
    expect(emitted![0].updatedAt).not.toBe('2024-01-01T00:00:00.000Z');
  });
});
