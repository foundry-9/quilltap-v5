import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { describe, expect, it } from 'vitest';

import type { CharacterScenario } from '../../../core/core-contract';
import { RichEditor } from '../../../editor/rich-editor';
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
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ScenarioEditor] });
  const fixture = TestBed.createComponent(ScenarioEditor);
  fixture.componentRef.setInput('scenarios', scenarios);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

/** The zoneless microtask loop — the editor mounts in afterNextRender. */
async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** The Archive / Restore button for row `index`. */
function archiveButton(
  fixture: ComponentFixture<ScenarioEditor>,
  index: number,
): HTMLButtonElement {
  const rows = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-card'),
  ) as HTMLElement[];
  const btn = Array.from(rows[index].querySelectorAll('button')).find((b) =>
    /^(Archive|Restore)$/.test((b.textContent ?? '').trim()),
  );
  if (!btn) throw new Error(`no archive button on row ${index}`);
  return btn as HTMLButtonElement;
}

/** The content editor handle for row `index`. */
function contentEditor(fixture: ComponentFixture<ScenarioEditor>, index = 0): RichEditor {
  return fixture.debugElement.queryAll(By.directive(RichEditor))[index]
    .componentInstance as RichEditor;
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

  it('updates title in place, bumping updatedAt', async () => {
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

  it('seeds each row s markdown field from its content', async () => {
    const fixture = await render([
      scenario({ id: 'a', content: 'The drawing room.' }),
      scenario({ id: 'b', content: 'The **conservatory**.' }),
    ]);
    expect(contentEditor(fixture, 0).getMarkdown()).toBe('The drawing room.');
    expect(contentEditor(fixture, 1).getMarkdown()).toBe('The **conservatory**.');
  });

  it('carries an edited row s markdown into the emitted array, bumping updatedAt', async () => {
    const fixture = await render([
      scenario({ id: 'a' }),
      scenario({ id: 'b', updatedAt: '2024-01-01T00:00:00.000Z' }),
    ]);
    let emitted: CharacterScenario[] | null = null;
    fixture.componentInstance.scenariosChange.subscribe((v) => (emitted = v));

    contentEditor(fixture, 1).setMarkdown('A *storm* breaks over the estate.');
    await settle(fixture);

    expect(emitted).not.toBeNull();
    expect(emitted![1].content).toBe('A *storm* breaks over the estate.');
    expect(emitted![1].updatedAt).not.toBe('2024-01-01T00:00:00.000Z');
    // The untouched row rides through byte-identical.
    expect(emitted![0].content).toBe('The drawing room, past midnight.');
  });

  /**
   * P4.D121 — archiving from the character edit form (v4 `d25dacc1`
   * `CharacterBasicInfo.tsx:455-486`). Two rules the vault depends on:
   * restoring DELETES the key (omission is what "active" means), and every
   * other field — including ones this editor never renders — rides through, or
   * the next projection sweep would drop it from the file.
   */
  it('archives a row by SETTING the key, and bumps only that row’s updatedAt', async () => {
    const fixture = await render([scenario({ id: 'a' }), scenario({ id: 'b' })]);
    let emitted: CharacterScenario[] | null = null;
    fixture.componentInstance.scenariosChange.subscribe((v) => (emitted = v));

    archiveButton(fixture, 0).click();
    await settle(fixture);

    expect(emitted![0].archived).toBe(true);
    expect(emitted![0].updatedAt).not.toBe('2024-01-01T00:00:00.000Z');
    expect('archived' in emitted![1]).toBe(false);
    expect(emitted![1].updatedAt).toBe('2024-01-01T00:00:00.000Z');
  });

  it('restoring DELETES the key rather than writing `archived: false`', async () => {
    const fixture = await render([scenario({ id: 'a', archived: true })]);
    let emitted: CharacterScenario[] | null = null;
    fixture.componentInstance.scenariosChange.subscribe((v) => (emitted = v));

    expect(archiveButton(fixture, 0).textContent!.trim()).toBe('Restore');
    archiveButton(fixture, 0).click();
    await settle(fixture);

    // The KEY is gone — not present-and-false. `archived: false` in the vault
    // file would be a dead flag v4 deliberately never writes.
    expect('archived' in emitted![0]).toBe(false);
    expect(JSON.stringify(emitted![0])).not.toContain('archived');
  });

  it('carries unrendered fields (description) through an archive toggle', async () => {
    const fixture = await render([scenario({ id: 'a', description: 'at dawn' })]);
    let emitted: CharacterScenario[] | null = null;
    fixture.componentInstance.scenariosChange.subscribe((v) => (emitted = v));
    archiveButton(fixture, 0).click();
    await settle(fixture);
    expect(emitted![0].description).toBe('at dawn');
  });

  it('badges an archived row and carries v4’s two button titles', async () => {
    const active = await render([scenario({ id: 'a' })]);
    expect((active.nativeElement as HTMLElement).textContent).not.toContain('Archived');
    expect(archiveButton(active, 0).textContent!.trim()).toBe('Archive');
    expect(archiveButton(active, 0).title).toBe('Hide this scenario from the chat pickers');

    const archived = await render([scenario({ id: 'a', archived: true })]);
    expect((archived.nativeElement as HTMLElement).textContent).toContain('Archived');
    expect(archiveButton(archived, 0).title).toBe('Restore this scenario to the chat pickers');
  });

  it('carries v4’s archiving help paragraph', async () => {
    const fixture = await render([scenario()]);
    const text = (fixture.nativeElement as HTMLElement).textContent!.replace(/\s+/g, ' ');
    expect(text).toContain(
      'Archiving a scenario keeps it here but hides it from the chat pickers unless “Show archived” is ticked there. Chats already using it are unaffected.',
    );
  });

  it('shows ALL scenarios, with no "Show archived" toggle of its own (by design)', async () => {
    const fixture = await render([
      scenario({ id: 'a', title: 'Active one' }),
      scenario({ id: 'b', title: 'Retired one', archived: true }),
    ]);
    const text = (fixture.nativeElement as HTMLElement).textContent!;
    expect(text).toContain('Archived');
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('input[type="checkbox"]'),
    ).toBeNull();
    expect(fixture.debugElement.queryAll(By.directive(RichEditor))).toHaveLength(2);
  });

  it('does not emit on load (the textarea contract the field replaces)', async () => {
    // __bold__ normalizes to **bold** on parse; absorb-once must swallow it, or
    // every mount would look like a user edit and dirty the parent form.
    const fixture = await render([scenario({ id: 'a', content: '__bold__' })]);
    let emitted: CharacterScenario[] | null = null;
    fixture.componentInstance.scenariosChange.subscribe((v) => (emitted = v));
    await settle(fixture);
    expect(emitted).toBeNull();
  });
});
