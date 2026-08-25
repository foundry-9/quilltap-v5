import { Component, signal } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { hasAnyScenarioOptions, ScenarioSelect } from './scenario-select';
import type {
  CharacterScenario,
  GeneralScenarioOption,
  GroupScenarioOption,
  ProjectScenarioOption,
  ScenarioSelection,
} from './scenario.types';

function projectScenario(over: Partial<ProjectScenarioOption> = {}): ProjectScenarioOption {
  return {
    path: 'Scenarios/inn.md',
    filename: 'inn.md',
    name: 'The Inn',
    isDefault: false,
    body: 'A low-ceilinged room.',
    ...over,
  };
}

function generalScenario(over: Partial<GeneralScenarioOption> = {}): GeneralScenarioOption {
  return {
    path: 'Scenarios/road.md',
    filename: 'road.md',
    name: 'The Road',
    isDefault: false,
    body: 'A road in the rain.',
    ...over,
  };
}

function groupScenario(over: Partial<GroupScenarioOption> = {}): GroupScenarioOption {
  return {
    groupId: 'grp-1',
    groupName: 'The Irregulars',
    path: 'Scenarios/den.md',
    filename: 'den.md',
    name: 'The Den',
    isDefault: false,
    body: 'A cellar.',
    ...over,
  };
}

/** A host so every input is a real binding (and can move between assertions). */
@Component({
  imports: [ScenarioSelect],
  template: `
    <qt-scenario-select
      selectId="probe-select"
      [selection]="selection()"
      [projectScenarios]="projectScenarios()"
      [generalScenarios]="generalScenarios()"
      [groupScenarios]="groupScenarios()"
      [characterScenarios]="characterScenarios()"
      [characterDefaultScenarioId]="characterDefaultScenarioId()"
      [disabled]="disabled()"
      [selectClass]="selectClass()"
      ariaLabel="Scenario"
      (selectionChange)="seen.push($event)"
    />
  `,
})
class Host {
  readonly selection = signal<ScenarioSelection>({ kind: 'custom' });
  readonly projectScenarios = signal<ProjectScenarioOption[]>([]);
  readonly generalScenarios = signal<GeneralScenarioOption[]>([]);
  readonly groupScenarios = signal<GroupScenarioOption[]>([]);
  readonly characterScenarios = signal<CharacterScenario[] | null>(null);
  readonly characterDefaultScenarioId = signal<string | null>(null);
  readonly disabled = signal(false);
  readonly selectClass = signal('qt-select mb-2');
  readonly seen: ScenarioSelection[] = [];
}

function mount(): ComponentFixture<Host> {
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  return fixture;
}

function select(fixture: ComponentFixture<Host>): HTMLSelectElement {
  return fixture.nativeElement.querySelector('select') as HTMLSelectElement;
}

function optionTexts(fixture: ComponentFixture<Host>): string[] {
  return [...select(fixture).querySelectorAll('option')].map((o) => o.textContent!.trim());
}

/**
 * Transcription parity for v4 `components/scenario/ScenarioSelect.tsx` at
 * `44a8137e` — the shared dropdown both the New Chat dialog and the Salon
 * sidebar's in-chat picker render.
 */
describe('ScenarioSelect (v4 components/scenario/ScenarioSelect.tsx @ 44a8137e)', () => {
  describe('hasAnyScenarioOptions', () => {
    it('is false only when every tier is empty or absent', () => {
      expect(hasAnyScenarioOptions({})).toBe(false);
      expect(
        hasAnyScenarioOptions({
          projectScenarios: [],
          generalScenarios: [],
          groupScenarios: [],
          characterScenarios: null,
        }),
      ).toBe(false);
      expect(hasAnyScenarioOptions({ projectScenarios: [projectScenario()] })).toBe(true);
      expect(hasAnyScenarioOptions({ generalScenarios: [generalScenario()] })).toBe(true);
      expect(hasAnyScenarioOptions({ groupScenarios: [groupScenario()] })).toBe(true);
      expect(
        hasAnyScenarioOptions({
          characterScenarios: [{ id: 'c1', title: 'A', content: 'x' }],
        }),
      ).toBe(true);
    });
  });

  it('renders only the standing Custom... entry when no tier has options', () => {
    const fixture = mount();
    expect(optionTexts(fixture)).toEqual(['Custom...']);
    expect(select(fixture).querySelectorAll('optgroup').length).toBe(0);
  });

  it('renders the four tiers as optgroups in v4’s order, Custom... first', () => {
    const fixture = mount();
    fixture.componentInstance.projectScenarios.set([projectScenario()]);
    fixture.componentInstance.generalScenarios.set([generalScenario()]);
    fixture.componentInstance.groupScenarios.set([groupScenario()]);
    fixture.componentInstance.characterScenarios.set([{ id: 'c1', title: 'A Duel', content: 'x' }]);
    fixture.detectChanges();

    expect([...select(fixture).querySelectorAll('optgroup')].map((g) => g.label)).toEqual([
      'Project Scenarios',
      'General Scenarios',
      'Group Scenarios: The Irregulars',
      'Character Scenarios',
    ]);
    expect(optionTexts(fixture)[0]).toBe('Custom...');
  });

  it('builds each option value from v4’s token format', () => {
    const fixture = mount();
    fixture.componentInstance.projectScenarios.set([projectScenario()]);
    fixture.componentInstance.generalScenarios.set([generalScenario()]);
    fixture.componentInstance.groupScenarios.set([groupScenario()]);
    fixture.componentInstance.characterScenarios.set([{ id: 'c1', title: 'A Duel', content: 'x' }]);
    fixture.detectChanges();

    expect([...select(fixture).querySelectorAll('option')].map((o) => o.value)).toEqual([
      '__custom__',
      'project:Scenarios/inn.md',
      'general:Scenarios/road.md',
      'group:grp-1:Scenarios/den.md',
      'c1',
    ]);
  });

  /**
   * v4 concatenates three adjacent JSX expressions with NOTHING between them.
   * The naive Angular translation puts them on separate template lines and
   * ships a stray space (memory `porting-a-react-component-to-angular` trap 2),
   * so these assert the whole rendered run, exactly.
   */
  it('labels a file option name + default suffix + description with no stray spaces', () => {
    const fixture = mount();
    fixture.componentInstance.projectScenarios.set([
      projectScenario({ isDefault: true, description: 'rain on the shutters' }),
      projectScenario({ path: 'Scenarios/ship.md', name: 'The Ship' }),
    ]);
    fixture.componentInstance.generalScenarios.set([
      generalScenario({ isDefault: true, description: 'mud' }),
    ]);
    fixture.componentInstance.groupScenarios.set([
      groupScenario({ isDefault: true, description: 'damp' }),
    ]);
    fixture.detectChanges();

    expect(optionTexts(fixture)).toEqual([
      'Custom...',
      'The Inn (project default) — rain on the shutters',
      'The Ship',
      'The Road (general default) — mud',
      'The Den (group default) — damp',
    ]);
  });

  it('labels a character option title + character default + description', () => {
    const fixture = mount();
    fixture.componentInstance.characterScenarios.set([
      { id: 'c1', title: 'A Duel', content: 'x', description: 'at dawn' },
      { id: 'c2', title: 'A Wake', content: 'y' },
    ]);
    fixture.componentInstance.characterDefaultScenarioId.set('c1');
    fixture.detectChanges();

    expect(optionTexts(fixture)).toEqual([
      'Custom...',
      'A Duel (character default) — at dawn',
      'A Wake',
    ]);
  });

  it('splits group scenarios into one optgroup per group, first sighting naming it', () => {
    const fixture = mount();
    fixture.componentInstance.groupScenarios.set([
      groupScenario({ groupId: 'g1', groupName: 'One', path: 'a.md', name: 'A' }),
      groupScenario({ groupId: 'g2', groupName: 'Two', path: 'b.md', name: 'B' }),
      groupScenario({ groupId: 'g1', groupName: 'One', path: 'c.md', name: 'C' }),
    ]);
    fixture.detectChanges();

    const groups = [...select(fixture).querySelectorAll('optgroup')];
    expect(groups.map((g) => g.label)).toEqual(['Group Scenarios: One', 'Group Scenarios: Two']);
    expect([...groups[0].querySelectorAll('option')].map((o) => o.textContent!.trim())).toEqual([
      'A',
      'C',
    ]);
  });

  it('reports the parsed selection on change (v4 `scenarioValueToSelection`)', () => {
    const fixture = mount();
    fixture.componentInstance.projectScenarios.set([projectScenario()]);
    fixture.detectChanges();

    const el = select(fixture);
    el.value = 'project:Scenarios/inn.md';
    el.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    expect(fixture.componentInstance.seen).toEqual([
      { kind: 'project', path: 'Scenarios/inn.md' },
    ]);
  });

  it('shows the selected row, and carries v4’s className + disabled through', () => {
    const fixture = mount();
    fixture.componentInstance.projectScenarios.set([projectScenario()]);
    fixture.componentInstance.selection.set({ kind: 'project', path: 'Scenarios/inn.md' });
    fixture.componentInstance.selectClass.set('qt-select text-sm mb-2');
    fixture.componentInstance.disabled.set(true);
    fixture.detectChanges();

    const el = select(fixture);
    expect(el.value).toBe('project:Scenarios/inn.md');
    expect(el.className).toBe('qt-select text-sm mb-2');
    expect(el.disabled).toBe(true);
    expect(el.id).toBe('probe-select');
    expect(el.getAttribute('aria-label')).toBe('Scenario');
  });

  /**
   * The one case that separates the three Angular mechanisms (memory
   * `angular-select-cannot-mirror-react-controlled-value`): React assigns
   * `select.value` AFTER the children mount, so a selection naming no option
   * leaves the control blank rather than silently snapping to row 0 ("Custom…").
   */
  it('leaves the control BLANK when the selection names no rendered option', () => {
    const fixture = mount();
    fixture.componentInstance.projectScenarios.set([projectScenario()]);
    fixture.componentInstance.selection.set({ kind: 'project', path: 'Scenarios/gone.md' });
    fixture.detectChanges();

    expect(select(fixture).selectedIndex).toBe(-1);
    expect(select(fixture).value).toBe('');
  });

  it('re-seats the selected row when the option list arrives after the selection', () => {
    const fixture = mount();
    fixture.componentInstance.selection.set({ kind: 'project', path: 'Scenarios/inn.md' });
    fixture.detectChanges();
    expect(select(fixture).selectedIndex).toBe(-1);

    fixture.componentInstance.projectScenarios.set([projectScenario()]);
    fixture.detectChanges();
    expect(select(fixture).value).toBe('project:Scenarios/inn.md');
  });

  /** The host must be a real box or `.qt-select`'s `w-full` has nothing to fill. */
  it('gives its host a block display', () => {
    const fixture = mount();
    const host = fixture.nativeElement.querySelector('qt-scenario-select') as HTMLElement;
    expect(host.classList.contains('block')).toBe(true);
  });
});
