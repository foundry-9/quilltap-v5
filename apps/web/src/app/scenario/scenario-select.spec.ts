import { Component, signal } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { ScenarioSelect } from './scenario-select';
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
      [offerBrowse]="offerBrowse()"
      (selectionChange)="seen.push($event)"
      (browse)="browsed = browsed + 1"
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
  readonly offerBrowse = signal(false);
  readonly seen: ScenarioSelection[] = [];
  browsed = 0;
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

  /**
   * P4.D121 — the archived marker (v4 `d25dacc1`). A native `<option>` cannot
   * hold a badge, so the marker is text, and it lands AFTER the default marker
   * and BEFORE the ` — description`.
   */
  it('marks an archived file option after the default marker, before the description', () => {
    const fixture = mount();
    fixture.componentInstance.projectScenarios.set([
      projectScenario({
        isDefault: true,
        archived: true,
        description: 'a cozy inn',
      }),
      projectScenario({ path: 'Scenarios/ship.md', name: 'The Ship', archived: true }),
      projectScenario({ path: 'Scenarios/keep.md', name: 'The Keep' }),
    ]);
    fixture.componentInstance.generalScenarios.set([generalScenario({ archived: true })]);
    fixture.componentInstance.groupScenarios.set([groupScenario({ archived: true })]);
    fixture.detectChanges();

    expect(optionTexts(fixture)).toEqual([
      'Custom...',
      'The Inn (project default) (archived) — a cozy inn',
      // Title order, leading "The" ignored (v4 1d0a31132): Inn, Keep, Ship.
      'The Keep',
      'The Ship (archived)',
      'The Road (archived)',
      'The Den (archived)',
    ]);
  });

  it('marks an archived character option, and an archived option stays selectable', () => {
    const fixture = mount();
    fixture.componentInstance.characterScenarios.set([
      { id: 'c1', title: 'A Duel', content: 'x', description: 'at dawn', archived: true },
      { id: 'c2', title: 'A Wake', content: 'y' },
    ]);
    fixture.componentInstance.characterDefaultScenarioId.set('c1');
    fixture.componentInstance.selection.set({ kind: 'character', scenarioId: 'c1' });
    fixture.detectChanges();

    expect(optionTexts(fixture)).toEqual([
      'Custom...',
      'A Duel (character default) (archived) — at dawn',
      'A Wake',
    ]);
    // Archiving hides an entry from the default view; it does not forbid one a
    // human has deliberately gone looking for.
    expect(select(fixture).value).toBe('c1');
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

const STORE: ScenarioSelection = {
  kind: 'store',
  mountPointId: 'mp-9',
  path: 'Scenarios/the-attic.md',
  label: 'Farhouse › The Attic',
};

/**
 * v4 `components/scenario/__tests__/scenario-selection.test.tsx` at the pin
 * `83334b9a5` (NEW at `545e299bb`) — the five "ScenarioSelect" cases,
 * transcribed, plus the two Angular-port rows the order names (L-B's
 * snap-back; the `afterRenderEffect` re-asserting a store selection).
 *
 * v4 offers "Browse all scenarios…" only when the surface passes `onBrowse`;
 * an Angular `output()` cannot be tested for a subscriber, so the entry rides
 * the `offerBrowse` input (P4.D302 L-A) and the browse is the `browse` output.
 */
describe('ScenarioSelect — the browse action and the Elsewhere optgroup (v4 545e299bb)', () => {
  function mountBrowsing(): ComponentFixture<Host> {
    const fixture = mount();
    fixture.componentInstance.offerBrowse.set(true);
    fixture.componentInstance.generalScenarios.set([
      generalScenario({ path: 'Scenarios/moor.md', filename: 'moor.md', name: 'The Moor', body: 'Fog.' }),
    ]);
    fixture.detectChanges();
    return fixture;
  }

  function choose(fixture: ComponentFixture<Host>, value: string): void {
    const el = select(fixture);
    el.value = value;
    el.dispatchEvent(new Event('change'));
    fixture.detectChanges();
  }

  it('calls onBrowse, not onChange, for the browse entry', () => {
    const fixture = mountBrowsing();
    choose(fixture, '__browse__');
    expect(fixture.componentInstance.browsed).toBe(1);
    expect(fixture.componentInstance.seen).toEqual([]);
  });

  it('reports an ordinary pick through onChange', () => {
    const fixture = mountBrowsing();
    choose(fixture, 'general:Scenarios/moor.md');
    expect(fixture.componentInstance.seen).toEqual([{ kind: 'general', path: 'Scenarios/moor.md' }]);
    expect(fixture.componentInstance.browsed).toBe(0);
  });

  it('omits the browse entry when the surface offers no browser', () => {
    const fixture = mountBrowsing();
    fixture.componentInstance.offerBrowse.set(false);
    fixture.detectChanges();
    const values = [...select(fixture).querySelectorAll('option')].map((o) => o.value);
    expect(values).not.toContain('__browse__');
    expect(values[0]).toBe('__custom__');
  });

  it('renders the Elsewhere optgroup holding exactly the store selection', () => {
    const fixture = mountBrowsing();
    fixture.componentInstance.selection.set(STORE);
    fixture.detectChanges();
    const elsewhere = select(fixture).querySelector('optgroup[label="Elsewhere"]');
    expect(elsewhere).not.toBeNull();
    const options = elsewhere!.querySelectorAll('option');
    expect(options).toHaveLength(1);
    expect(options[0].textContent!.trim()).toBe('Farhouse › The Attic');
    expect(select(fixture).value).toBe('store:mp-9:Scenarios/the-attic.md');
  });

  it('has no Elsewhere optgroup for a native selection', () => {
    const fixture = mountBrowsing();
    fixture.componentInstance.selection.set({ kind: 'general', path: 'Scenarios/moor.md' });
    fixture.detectChanges();
    expect(select(fixture).querySelector('optgroup[label="Elsewhere"]')).toBeNull();
  });

  it('places Custom..., then Browse all scenarios…, then Elsewhere, then the tiers', () => {
    const fixture = mountBrowsing();
    fixture.componentInstance.selection.set(STORE);
    fixture.detectChanges();
    expect(optionTexts(fixture)).toEqual([
      'Custom...',
      'Browse all scenarios…',
      'Farhouse › The Attic',
      'The Moor',
    ]);
  });

  /**
   * L-B — v4's controlled `<select>` snaps back on React's next render. v5's
   * `afterRenderEffect` re-assigns the value only when a TRACKED signal moves,
   * and choosing Browse moves none, so the handler restores it explicitly
   * (else the DOM would keep showing "Browse all scenarios…").
   */
  it('snaps the select back to the held selection after the browse entry is chosen', () => {
    const fixture = mountBrowsing();
    fixture.componentInstance.selection.set({ kind: 'general', path: 'Scenarios/moor.md' });
    fixture.detectChanges();
    choose(fixture, '__browse__');
    expect(select(fixture).value).toBe('general:Scenarios/moor.md');

    fixture.componentInstance.selection.set({ kind: 'custom' });
    fixture.detectChanges();
    choose(fixture, '__browse__');
    expect(select(fixture).value).toBe('__custom__');
  });

  it('passes the held selection as `current`, so a re-picked store keeps its label', () => {
    const fixture = mountBrowsing();
    fixture.componentInstance.selection.set(STORE);
    fixture.detectChanges();
    choose(fixture, 'store:mp-9:Scenarios/the-attic.md');
    expect(fixture.componentInstance.seen).toEqual([STORE]);
  });

  it('re-asserts a store selection after a tier refetch moves the rows', () => {
    const fixture = mountBrowsing();
    fixture.componentInstance.selection.set(STORE);
    fixture.detectChanges();
    fixture.componentInstance.generalScenarios.set([
      generalScenario({ path: 'Scenarios/heath.md', name: 'The Heath' }),
      generalScenario({ path: 'Scenarios/moor.md', name: 'The Moor' }),
    ]);
    fixture.detectChanges();
    expect(select(fixture).value).toBe('store:mp-9:Scenarios/the-attic.md');
  });

  it('renders the Custom... and Browse entries even when no tier has anything to offer', () => {
    const fixture = mount();
    fixture.componentInstance.offerBrowse.set(true);
    fixture.detectChanges();
    expect([...select(fixture).querySelectorAll('option')].map((o) => o.value)).toEqual([
      '__custom__',
      '__browse__',
    ]);
  });
});

describe('ScenarioSelect — every optgroup in title order (v4 1d0a31132)', () => {
  /** An out-of-order list: path order differs from title order, with articles. */
  const ordered = ['Atrium', 'The Ballroom', 'casino', 'The Zeppelin'];
  const shuffled = ['The Zeppelin', 'casino', 'Atrium', 'The Ballroom'];
  const file = (name: string) => ({
    path: `Scenarios/${name.toLowerCase().replace(/\s+/g, '-')}.md`,
    filename: name.toLowerCase().replace(/\s+/g, '-'),
    name,
  });

  function groupTexts(fixture: ComponentFixture<Host>, label: string): string[] {
    const group = [...select(fixture).querySelectorAll('optgroup')].find((g) => g.label === label);
    return [...group!.querySelectorAll('option')].map((o) => o.textContent!.trim());
  }

  it('sorts the project, general, group and character optgroups', () => {
    const fixture = mount();
    const host = fixture.componentInstance;
    host.projectScenarios.set(shuffled.map((n) => projectScenario(file(n))));
    host.generalScenarios.set(shuffled.map((n) => generalScenario(file(n))));
    host.groupScenarios.set(
      ['Zed', 'The Apple'].map((n) => groupScenario({ ...file(n), groupId: 'g-1', groupName: 'Guild' })),
    );
    host.characterScenarios.set(
      shuffled.map((title, i) => ({ id: `c${i}`, title, content: 'x' })),
    );
    fixture.detectChanges();

    expect(groupTexts(fixture, 'Project Scenarios')).toEqual(ordered);
    expect(groupTexts(fixture, 'General Scenarios')).toEqual(ordered);
    expect(groupTexts(fixture, 'Group Scenarios: Guild')).toEqual(['The Apple', 'Zed']);
    expect(groupTexts(fixture, 'Character Scenarios')).toEqual(ordered);
  });

  it('keeps the GROUP order at first sighting while sorting inside each group', () => {
    const fixture = mount();
    fixture.componentInstance.groupScenarios.set([
      groupScenario({ ...file('Zed'), groupId: 'g-z', groupName: 'Zebras' }),
      groupScenario({ ...file('Beta'), groupId: 'g-a', groupName: 'Ants' }),
      groupScenario({ ...file('Alpha'), groupId: 'g-z', groupName: 'Zebras' }),
    ]);
    fixture.detectChanges();

    const groups = [...select(fixture).querySelectorAll('optgroup')];
    expect(groups.map((g) => g.label)).toEqual(['Group Scenarios: Zebras', 'Group Scenarios: Ants']);
    expect([...groups[0].querySelectorAll('option')].map((o) => o.textContent!.trim())).toEqual([
      'Alpha',
      'Zed',
    ]);
  });

  it('lands an appended "fresh save" in its place, not at the end', () => {
    const fixture = mount();
    const host = fixture.componentInstance;
    host.projectScenarios.set(['Atrium', 'casino'].map((n) => projectScenario(file(n))));
    fixture.detectChanges();
    host.projectScenarios.update((list) => [...list, projectScenario(file('The Ballroom'))]);
    fixture.detectChanges();

    expect(groupTexts(fixture, 'Project Scenarios')).toEqual(['Atrium', 'The Ballroom', 'casino']);
  });
});
