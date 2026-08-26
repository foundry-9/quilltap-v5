import { ComponentFixture, TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { By } from '@angular/platform-browser';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ScenarioDto } from '../../../core/core-contract';
import { RichEditor } from '../../../editor/rich-editor';
import type { ScenarioMutator, ScenarioResult } from '../scenarios.api';
import { ScenariosManager } from './scenarios-manager';

function scenario(over: Partial<ScenarioDto> = {}): ScenarioDto {
  return {
    path: 'Scenarios/welcome.md',
    filename: 'welcome',
    name: 'Welcome to the Estate',
    description: 'A summer evening.',
    isDefault: false,
    rawIsDefault: false,
    archived: false,
    body: 'The gates swing wide for {{user}}.',
    lastModified: '2024-01-01T00:00:00.000Z',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

interface MockHandle {
  mutator: ScenarioMutator;
  calls: {
    create: unknown[];
    update: [string, unknown][];
    rename: [string, string][];
    delete: string[];
    setDefault: string[];
    archived: [string, boolean][];
    showArchived: boolean[];
  };
  scenarios: ReturnType<typeof signal<ScenarioDto[]>>;
  showArchived: ReturnType<typeof signal<boolean>>;
  results: {
    create: ScenarioResult<{ path: string }>;
    update: ScenarioResult;
    rename: ScenarioResult<{ path: string }>;
    delete: ScenarioResult;
    setDefault: ScenarioResult;
  };
}

function mockMutator(init: Partial<Record<'scenarios' | 'warnings', unknown>> = {}): MockHandle {
  const scenarios = signal<ScenarioDto[]>((init.scenarios as ScenarioDto[]) ?? []);
  const warnings = signal<string[]>((init.warnings as string[]) ?? []);
  const loading = signal(false);
  const error = signal<string | null>(null);
  const mountPointId = signal<string | null>('mount-1');
  const showArchived = signal(false);
  const calls: MockHandle['calls'] = {
    create: [],
    update: [],
    rename: [],
    delete: [],
    setDefault: [],
    archived: [],
    showArchived: [],
  };
  const results: MockHandle['results'] = {
    create: { ok: true, path: 'Scenarios/x.md' },
    update: { ok: true },
    rename: { ok: true, path: 'Scenarios/x.md' },
    delete: { ok: true },
    setDefault: { ok: true },
  };
  const mutator: ScenarioMutator = {
    scenarios,
    warnings,
    loading,
    error,
    mountPointId,
    showArchived,
    setShowArchived: (next) => {
      calls.showArchived.push(next);
      showArchived.set(next);
    },
    refresh: async () => undefined,
    createScenario: async (input) => {
      calls.create.push(input);
      return results.create;
    },
    updateScenario: async (path, input) => {
      calls.update.push([path, input]);
      return results.update;
    },
    renameScenario: async (path, newFilename) => {
      calls.rename.push([path, newFilename]);
      return results.rename;
    },
    deleteScenario: async (path) => {
      calls.delete.push(path);
      return results.delete;
    },
    setDefaultScenario: async (path) => {
      calls.setDefault.push(path);
      return results.setDefault;
    },
    setScenarioArchived: async (path, archived) => {
      calls.archived.push([path, archived]);
      return results.update;
    },
  };
  return { mutator, calls, scenarios, results, showArchived };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 4): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

async function render(
  handle: MockHandle,
  opts: { scopeLabel?: string; emptyMessage?: string } = {},
): Promise<ComponentFixture<ScenariosManager>> {
  TestBed.configureTestingModule({ imports: [ScenariosManager] });
  const fixture = TestBed.createComponent(ScenariosManager);
  fixture.componentRef.setInput('mutator', handle.mutator);
  if (opts.scopeLabel) fixture.componentRef.setInput('scopeLabel', opts.scopeLabel);
  if (opts.emptyMessage) fixture.componentRef.setInput('emptyMessage', opts.emptyMessage);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function byText(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  const btn = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
  ).find((b) => (b.textContent ?? '').trim() === label);
  if (!btn) throw new Error(`button "${label}" not found`);
  return btn as HTMLButtonElement;
}

function setInput(el: HTMLElement, selector: string, value: string): void {
  const input = el.querySelector(selector) as HTMLInputElement | HTMLTextAreaElement;
  input.value = value;
  input.dispatchEvent(new Event('input'));
}

/** The modal's body editor (qt-markdown-field's RichEditor handle) — drive it
 *  imperatively rather than through DOM input events. */
function bodyEditor(fixture: ComponentFixture<unknown>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

/** Whether the modal (and so its body editor) is on screen. */
function modalOpen(fixture: ComponentFixture<unknown>): boolean {
  return !!(fixture.nativeElement as HTMLElement).querySelector('qt-markdown-field');
}

describe('ScenariosManager', () => {
  afterEach(() => vi.restoreAllMocks());

  it('shows the default empty message with no scenarios', async () => {
    const fixture = await render(mockMutator());
    expect(text(fixture)).toContain(
      "No scenarios yet. Create one and it'll be offered when starting new chats.",
    );
  });

  it('honors a scope-specific empty message', async () => {
    const fixture = await render(mockMutator(), { emptyMessage: 'No general scenarios yet.' });
    expect(text(fixture)).toContain('No general scenarios yet.');
  });

  it('renders a row per scenario with the .md filename and Default badge', async () => {
    const handle = mockMutator({
      scenarios: [scenario({ isDefault: true }), scenario({ path: 'Scenarios/duel.md', filename: 'duel', name: 'A Duel at Dawn' })],
    });
    const fixture = await render(handle);
    const rows = (fixture.nativeElement as HTMLElement).querySelectorAll('qt-scenario-row');
    expect(rows.length).toBe(2);
    expect(text(fixture)).toContain('welcome.md');
    expect(text(fixture)).toContain('A Duel at Dawn');
    expect(text(fixture)).toContain('Default');
  });

  it('surfaces the mutator warnings block', async () => {
    const fixture = await render(mockMutator({ warnings: ['Two files are marked default.'] }));
    expect(fixture.nativeElement.querySelector('.qt-alert-warning')).toBeTruthy();
    expect(text(fixture)).toContain('Two files are marked default.');
  });

  it('opens the create modal titled "New scenario" with a filename field', async () => {
    const fixture = await render(mockMutator());
    byText(fixture, '+ New scenario').click();
    await settle(fixture);
    expect(text(fixture)).toContain('New scenario');
    expect(fixture.nativeElement.querySelector('#scenario-filename')).toBeTruthy();
  });

  it('creates: routes the full bag through createScenario and closes', async () => {
    const handle = mockMutator();
    const fixture = await render(handle, { scopeLabel: 'project' });
    byText(fixture, '+ New scenario').click();
    await settle(fixture);

    const el = fixture.nativeElement as HTMLElement;
    setInput(el, '#scenario-filename', 'garden-party');
    setInput(el, '#scenario-name', 'Garden Party');
    setInput(el, '#scenario-description', 'Tea on the lawn.');
    bodyEditor(fixture).setMarkdown('The lawn is set for {{char}}.');
    await settle(fixture);
    byText(fixture, 'Create scenario').click();
    await settle(fixture);

    expect(handle.calls.create).toEqual([
      {
        filename: 'garden-party',
        name: 'Garden Party',
        description: 'Tea on the lawn.',
        isDefault: false,
        body: 'The lawn is set for {{char}}.',
      },
    ]);
    // Modal closed on success.
    expect(modalOpen(fixture)).toBe(false);
  });

  it('blocks create with empty body / name / filename microcopy', async () => {
    const fixture = await render(mockMutator());
    const el = fixture.nativeElement as HTMLElement;

    byText(fixture, '+ New scenario').click();
    await settle(fixture);
    byText(fixture, 'Create scenario').click();
    await settle(fixture);
    expect(text(fixture)).toContain('Scenario body cannot be empty.');

    bodyEditor(fixture).setMarkdown('A body.');
    await settle(fixture);
    byText(fixture, 'Create scenario').click();
    await settle(fixture);
    expect(text(fixture)).toContain('Scenario must have a name.');

    setInput(el, '#scenario-name', 'A Name');
    await settle(fixture);
    byText(fixture, 'Create scenario').click();
    await settle(fixture);
    expect(text(fixture)).toContain('Filename is required.');
  });

  it('edits: modal drops the filename field and updateScenario carries no filename', async () => {
    const handle = mockMutator({ scenarios: [scenario()] });
    const fixture = await render(handle);
    const el = fixture.nativeElement as HTMLElement;

    byText(fixture, 'Edit').click();
    await settle(fixture);
    expect(text(fixture)).toContain('Edit scenario — Welcome to the Estate');
    expect(el.querySelector('#scenario-filename')).toBeFalsy();
    // The edit modal seeds the field from the stored body.
    expect(bodyEditor(fixture).getMarkdown()).toBe('The gates swing wide for {{user}}.');

    bodyEditor(fixture).setMarkdown('A new body.');
    await settle(fixture);
    byText(fixture, 'Save changes').click();
    await settle(fixture);

    expect(handle.calls.update).toEqual([
      [
        'Scenarios/welcome.md',
        {
          name: 'Welcome to the Estate',
          description: 'A summer evening.',
          isDefault: false,
          body: 'A new body.',
        },
      ],
    ]);
  });

  it('re-saves an untouched body byte-identical (the markdown field is not an edit)', async () => {
    // Emphasis markers and the {{user}} placeholder must survive a parse /
    // serialize round-trip through the editor untouched — a save with no user
    // edit must not rewrite the stored file.
    const body = 'The gates swing wide for {{user}}.\n\nA *summer* evening, and **no rain**.';
    const handle = mockMutator({ scenarios: [scenario({ body })] });
    const fixture = await render(handle);

    byText(fixture, 'Edit').click();
    await settle(fixture);
    expect(bodyEditor(fixture).getMarkdown()).toBe(body);

    byText(fixture, 'Save changes').click();
    await settle(fixture);

    expect(handle.calls.update).toEqual([
      [
        'Scenarios/welcome.md',
        { name: 'Welcome to the Estate', description: 'A summer evening.', isDefault: false, body },
      ],
    ]);
  });

  it('deletes only when the confirm is accepted', async () => {
    const handle = mockMutator({ scenarios: [scenario()] });
    const fixture = await render(handle);

    vi.spyOn(window, 'confirm').mockReturnValue(false);
    byText(fixture, 'Delete').click();
    await settle(fixture);
    expect(handle.calls.delete).toEqual([]);

    vi.spyOn(window, 'confirm').mockReturnValue(true);
    byText(fixture, 'Delete').click();
    await settle(fixture);
    expect(handle.calls.delete).toEqual(['Scenarios/welcome.md']);
  });

  it('renames on the FILENAME, and no-ops when unchanged or empty', async () => {
    const handle = mockMutator({ scenarios: [scenario()] });
    const fixture = await render(handle);
    const promptSpy = vi.spyOn(window, 'prompt');

    // Unchanged → no call.
    promptSpy.mockReturnValueOnce('welcome');
    byText(fixture, 'Rename').click();
    await settle(fixture);
    // Cancelled → no call.
    promptSpy.mockReturnValueOnce(null);
    byText(fixture, 'Rename').click();
    await settle(fixture);
    expect(handle.calls.rename).toEqual([]);
    // The prompt is prefilled with the current filename.
    expect(promptSpy).toHaveBeenCalledWith('Rename scenario "welcome" to:', 'welcome');

    promptSpy.mockReturnValueOnce('  welcome-back  ');
    byText(fixture, 'Rename').click();
    await settle(fixture);
    expect(handle.calls.rename).toEqual([['Scenarios/welcome.md', 'welcome-back']]);
  });

  it('sets default via re-send; no-ops on an already-default row', async () => {
    const handle = mockMutator({
      scenarios: [scenario({ isDefault: true }), scenario({ path: 'Scenarios/duel.md', filename: 'duel' })],
    });
    const fixture = await render(handle);
    const radios = (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLInputElement>(
      'input[type="radio"]',
    );
    // Clicking the already-default row's radio is a no-op.
    radios[0].dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(handle.calls.setDefault).toEqual([]);
    // Clicking the non-default row re-sends update.
    radios[1].dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(handle.calls.setDefault).toEqual(['Scenarios/duel.md']);
  });

  /**
   * P4.D121 — the archive surface (v4 `d25dacc1`): the "Show archived" checkbox
   * flips the mutator's FETCH; Archive/Restore is single-click with NO confirm;
   * a failure lands in the action-error banner like every other mutation.
   */
  it('“Show archived” flips the mutator’s fetch, never a client-side filter', async () => {
    const handle = mockMutator({ scenarios: [scenario()] });
    const fixture = await render(handle);
    const box = (fixture.nativeElement as HTMLElement).querySelector(
      'input[type="checkbox"].qt-checkbox',
    ) as HTMLInputElement;
    expect(box.checked).toBe(false);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(handle.calls.showArchived).toEqual([true]);
    expect(handle.scenarios()).toHaveLength(1);
  });

  it('archives and restores without a confirm dialog', async () => {
    const handle = mockMutator({ scenarios: [scenario()] });
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fixture = await render(handle);

    byText(fixture, 'Archive').click();
    await settle(fixture);
    expect(handle.calls.archived).toEqual([['Scenarios/welcome.md', true]]);
    expect(confirm).not.toHaveBeenCalled();

    handle.scenarios.set([scenario({ archived: true })]);
    await settle(fixture);
    byText(fixture, 'Restore').click();
    await settle(fixture);
    expect(handle.calls.archived).toEqual([
      ['Scenarios/welcome.md', true],
      ['Scenarios/welcome.md', false],
    ]);
  });

  it('surfaces a failed archive as an action error', async () => {
    const handle = mockMutator({ scenarios: [scenario()] });
    handle.results.update = { ok: false, error: 'Scenario not found in current list' };
    const fixture = await render(handle);
    byText(fixture, 'Archive').click();
    await settle(fixture);
    expect(text(fixture)).toContain('Scenario not found in current list');
  });

  it('surfaces a failed mutation as an action error', async () => {
    const handle = mockMutator({ scenarios: [scenario()] });
    handle.results.delete = { ok: false, error: 'A scenario named "welcome" already exists' };
    const fixture = await render(handle);
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    byText(fixture, 'Delete').click();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
    expect(text(fixture)).toContain('A scenario named "welcome" already exists');
  });
});
