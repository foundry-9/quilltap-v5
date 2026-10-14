import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import {
  CoreDispatchError,
  type EverywhereScenario,
  type EverywhereSource,
} from '../core/core-contract';
import { ScenarioBrowserDialog } from './scenario-browser-dialog';
import type { ScenarioHierarchy } from './scenario-browser.logic';
import type { ScenarioSelection } from './scenario.types';

/**
 * The scenario browser — "Every scene on every shelf" (v4
 * `components/scenario/ScenarioBrowserDialog.tsx` `:151-399` at the pin
 * `83334b9a5`, NEW at `545e299bb`). v4 carries no jsdom file for the dialog
 * itself (its pure helpers are pinned in `scenario-selection.test.tsx`, here
 * by the recorded oracle); these rows pin every string and keyboard rule of
 * the order's item 6 against v4's source.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

function scenario(over: Partial<EverywhereScenario> = {}): EverywhereScenario {
  return {
    path: 'Scenarios/the-attic.md',
    filename: 'the-attic.md',
    name: 'The Attic',
    isDefault: false,
    archived: false,
    excerpt: 'Dust, trunks and a single round window.',
    updatedAt: '2026-10-01T00:00:00.000Z',
    ...over,
  };
}

const GENERAL: EverywhereSource = {
  scope: 'general',
  id: null,
  name: 'Quilltap General',
  mountPointId: 'mp-gen',
  scenarios: [scenario({ path: 'Scenarios/moor.md', name: 'The Moor', excerpt: 'Fog.', isDefault: true })],
  warnings: [],
};
const OWN_PROJECT: EverywhereSource = {
  scope: 'project',
  id: 'proj-1',
  name: 'Glasshouse',
  mountPointId: 'mp-proj-1',
  scenarios: [scenario({ path: 'Scenarios/atrium.md', name: 'The Atrium', excerpt: 'Glass overhead.' })],
  warnings: [],
};
const OTHER_PROJECT: EverywhereSource = {
  scope: 'project',
  id: 'proj-2',
  name: 'Farhouse',
  mountPointId: 'mp-far',
  scenarios: [
    scenario({ description: 'A forgotten room' }),
    scenario({ path: 'Scenarios/cellar.md', name: 'The Cellar', excerpt: 'Damp stone.' }),
  ],
  warnings: [],
};
const EMPTY_PROJECT: EverywhereSource = {
  scope: 'project',
  id: 'proj-3',
  name: 'Bare Walls',
  mountPointId: 'mp-bare',
  scenarios: [],
  warnings: [],
};
const ARCHIVED_ONLY: EverywhereSource = {
  ...OTHER_PROJECT,
  scenarios: [
    ...OTHER_PROJECT.scenarios,
    scenario({ path: 'Scenarios/old.md', name: 'The Old Wing', archived: true }),
  ],
};

const HIERARCHY: ScenarioHierarchy = { projectId: 'proj-1', groupIds: [], characterId: null };

interface Stub {
  sent: Req[];
  everywhere: (includeArchived: boolean) => Promise<unknown>;
  read: (ref: { mountPointId: string; path: string }) => Promise<unknown>;
}

let stub: Stub;

function defaultStub(): Stub {
  return {
    sent: [],
    everywhere: async (archived) => ({
      sources: archived
        ? [GENERAL, OWN_PROJECT, ARCHIVED_ONLY, EMPTY_PROJECT]
        : [GENERAL, OWN_PROJECT, OTHER_PROJECT, EMPTY_PROJECT],
    }),
    read: async (ref) => ({
      scenario: { body: `# Body of ${ref.path}\n\nThe *full* scene.` },
      owner: {},
    }),
  };
}

const core = {
  dispatchData: async (req: Req) => {
    stub.sent.push(req);
    if (req.type === 'scenarioEverywhere') return stub.everywhere(req['includeArchived'] === true);
    if (req.type === 'scenarioReadByRef') {
      return stub.read({ mountPointId: req['mountPointId'] as string, path: req['path'] as string });
    }
    return {};
  },
} as unknown as CoreClient;

@Component({
  imports: [ScenarioBrowserDialog],
  template: `
    @if (open()) {
      <qt-scenario-browser-dialog
        [hierarchy]="hierarchy()"
        (closed)="onClosed()"
        (pick)="picks.push($event)"
      />
    }
  `,
})
class Host {
  readonly open = signal(true);
  readonly hierarchy = signal<ScenarioHierarchy>(HIERARCHY);
  readonly picks: ScenarioSelection[] = [];
  closes = 0;
  onClosed(): void {
    this.closes += 1;
    this.open.set(false);
  }
}

let client: QueryClient;

async function render(s: Stub = defaultStub()): Promise<ComponentFixture<Host>> {
  stub = s;
  // The app's global defaults (`app.config.ts:12-18`: `staleTime: 5_000`), so
  // the browser's own `staleTime: 0` is what a remount refetch rides on.
  client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 5_000 } } });
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [{ provide: CoreClient, useValue: core }, provideTanStackQuery(client)],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

/**
 * Macrotask turns + change detection. Not `whenStable()`: TanStack registers an
 * in-flight query as a pending task, so a spec holding a read open on purpose
 * (the loading rows) would wait forever.
 */
async function settle(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** The dialog is portalled to `document.body`, so read it from there. */
function dialog(): HTMLElement {
  return document.body.querySelector('qt-scenario-browser-dialog') as HTMLElement;
}
const q = <T extends Element = HTMLElement>(sel: string): T | null => dialog()?.querySelector<T>(sel) ?? null;
const qa = <T extends Element = HTMLElement>(sel: string): T[] => [...(dialog()?.querySelectorAll<T>(sel) ?? [])];
const text = (): string => (dialog()?.textContent ?? '').replace(/\s+/g, ' ');
const rows = (): HTMLLIElement[] => qa<HTMLLIElement>('[role="listbox"] [role="option"]');
const rowNames = (): string[] => rows().map((r) => r.querySelector('.font-medium')!.textContent!.trim());
const button = (label: string): HTMLButtonElement =>
  [...(dialog()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find(
    (b) => b.textContent!.replace(/\s+/g, ' ').trim() === label,
  )!;
const ownerButton = (name: string): HTMLButtonElement =>
  qa<HTMLButtonElement>('nav li button').find((b) => b.querySelector('.truncate')!.textContent!.trim() === name)!;

function key(fixture: ComponentFixture<Host>, k: string): KeyboardEvent {
  const target = q('input[type="search"]') ?? dialog();
  const event = new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true });
  target!.dispatchEvent(event);
  fixture.detectChanges();
  return event;
}

function typeSearch(fixture: ComponentFixture<Host>, value: string): void {
  const input = q<HTMLInputElement>('input[type="search"]')!;
  input.value = value;
  input.dispatchEvent(new Event('input'));
  fixture.detectChanges();
}

afterEach(() => {
  document.body.querySelectorAll('qt-scenario-browser-dialog').forEach((n) => n.remove());
});

describe('ScenarioBrowserDialog — the frame (v4 545e299bb)', () => {
  it('renders v4’s title, search box and Show archived', async () => {
    await render();
    expect(q('.qt-dialog-title')!.textContent!.trim()).toBe('Every scene on every shelf');
    const search = q<HTMLInputElement>('input[type="search"]')!;
    expect(search.getAttribute('placeholder')).toBe('Search every shelf…');
    expect(search.getAttribute('aria-label')).toBe('Search scenarios');
    expect(text()).toContain('Show archived');
    expect(q('.qt-dialog')!.getAttribute('style')).toContain('max-width: 56rem');
  });

  it('autofocuses the search box', async () => {
    await render();
    expect(document.activeElement).toBe(q('input[type="search"]'));
  });

  it('moves itself to document.body (the BaseModal portal)', async () => {
    const fixture = await render();
    expect(dialog().parentElement).toBe(document.body);
    expect((fixture.nativeElement as HTMLElement).querySelector('qt-scenario-browser-dialog')).toBeNull();
    fixture.componentInstance.open.set(false);
    fixture.detectChanges();
    expect(dialog()).toBeNull();
  });

  it('shows "Dusting the shelves…" while the catalogue loads, then the shelves', async () => {
    let release!: (v: unknown) => void;
    const s = defaultStub();
    const real = s.everywhere;
    s.everywhere = (a) => new Promise((r) => (release = () => r(real(a))));
    const fixture = await render(s);
    expect(text()).toContain('Dusting the shelves…');
    expect(q('nav')).toBeNull();
    release(undefined);
    await settle(fixture);
    expect(text()).not.toContain('Dusting the shelves…');
    expect(q('nav[aria-label="Scenario owners"]')).not.toBeNull();
  });

  it('shows the error line when the shelves cannot be reached', async () => {
    const s = defaultStub();
    s.everywhere = async () => {
      throw new CoreDispatchError({ kind: 'badRequest', message: 'unknown variant `scenarioEverywhere`' } as never);
    };
    await render(s);
    expect(q('p.qt-text-destructive')!.textContent!.trim()).toBe(
      'The shelves could not be reached: unknown variant `scenarioEverywhere`',
    );
    expect(q('nav')).toBeNull();
  });

  it('closes on Cancel and on the ✕', async () => {
    const fixture = await render();
    button('Cancel').click();
    fixture.detectChanges();
    expect(fixture.componentInstance.closes).toBe(1);
    expect(dialog()).toBeNull();

    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    await settle(fixture);
    (q('button[aria-label="Close"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(fixture.componentInstance.closes).toBe(2);
  });
});

describe('ScenarioBrowserDialog — the owners nav', () => {
  it('lists Every shelf, then only the sections that have owners, in v4’s order', async () => {
    await render();
    const nav = q('nav')!;
    const heads = [...nav.querySelectorAll('button[aria-expanded]')].map((b) => b.textContent!.trim());
    expect(nav.querySelector('button')!.textContent!.trim()).toBe('Every shelf');
    expect(heads).toEqual(['▾ General', '▾ Projects']);
  });

  it('collapses a section with ▸ and aria-expanded=false, and reopens it', async () => {
    const fixture = await render();
    const head = () => qa<HTMLButtonElement>('nav button[aria-expanded]')[1];
    expect(head().getAttribute('aria-expanded')).toBe('true');
    head().click();
    fixture.detectChanges();
    expect(head().textContent!.trim()).toBe('▸ Projects');
    expect(head().getAttribute('aria-expanded')).toBe('false');
    expect(ownerButton('Glasshouse')).toBeUndefined();
    head().click();
    fixture.detectChanges();
    expect(ownerButton('Glasshouse')).toBeDefined();
  });

  it('shows each owner’s count, greys an empty one with v4’s title', async () => {
    await render();
    const empty = ownerButton('Bare Walls');
    expect(empty.getAttribute('title')).toBe('Nothing filed here yet.');
    expect(empty.className).toContain('opacity-60');
    const spans = (b: HTMLButtonElement) => [...b.querySelectorAll('span')].map((x) => x.textContent!.trim());
    expect(spans(empty)).toEqual(['Bare Walls', '0']);
    expect(ownerButton('Farhouse').getAttribute('title')).toBeNull();
    expect(spans(ownerButton('Farhouse'))).toEqual(['Farhouse', '2']);
  });

  it('badges the owners the chat already offers "in this chat"', async () => {
    await render();
    expect(ownerButton('Quilltap General').textContent).toContain('in this chat');
    expect(ownerButton('Glasshouse').textContent).toContain('in this chat');
    expect(ownerButton('Farhouse').textContent).not.toContain('in this chat');
  });
});

describe('ScenarioBrowserDialog — the results', () => {
  it('lists every shelf, with owner names, when no owner is selected', async () => {
    await render();
    expect(q('[role="listbox"]')!.getAttribute('aria-label')).toBe('Scenarios');
    expect(rowNames()).toEqual(['The Moor', 'The Atrium', 'The Attic', 'The Cellar']);
    expect(rows()[0].textContent).toContain('Quilltap General');
    expect(rows()[0].querySelector('.qt-badge-secondary')!.textContent!.trim()).toBe('default');
    expect(rows()[2].textContent).toContain('A forgotten room');
  });

  it('narrows to the selected owner, without owner names', async () => {
    const fixture = await render();
    ownerButton('Farhouse').click();
    fixture.detectChanges();
    expect(rowNames()).toEqual(['The Attic', 'The Cellar']);
    expect(rows()[0].textContent).not.toContain('Farhouse');
    button('Every shelf').click();
    fixture.detectChanges();
    expect(rowNames()).toHaveLength(4);
  });

  it('searches EVERY shelf whatever owner is selected, owner names shown', async () => {
    const fixture = await render();
    ownerButton('Glasshouse').click();
    fixture.detectChanges();
    typeSearch(fixture, 'forgotten');
    expect(rowNames()).toEqual(['The Attic']);
    expect(rows()[0].textContent).toContain('Farhouse');
  });

  it('says "Nothing on that shelf answers to the name." for a search miss', async () => {
    const fixture = await render();
    typeSearch(fixture, 'conservatory');
    expect(q('[role="listbox"]')).toBeNull();
    expect(text()).toContain('Nothing on that shelf answers to the name.');
  });

  it('says "Nothing filed here yet." for an empty owner', async () => {
    const fixture = await render();
    ownerButton('Bare Walls').click();
    fixture.detectChanges();
    expect(text()).toContain('Nothing filed here yet.');
  });

  it('refetches the catalogue with includeArchived on Show archived, as a SEPARATE cache entry', async () => {
    const fixture = await render();
    expect(stub.sent.filter((r) => r.type === 'scenarioEverywhere')).toEqual([
      { type: 'scenarioEverywhere', includeArchived: false },
    ]);
    const box = qa<HTMLInputElement>('input[type="checkbox"]')[0];
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    await settle(fixture);
    expect(stub.sent.filter((r) => r.type === 'scenarioEverywhere')).toEqual([
      { type: 'scenarioEverywhere', includeArchived: false },
      { type: 'scenarioEverywhere', includeArchived: true },
    ]);
    expect(rows().find((r) => r.textContent!.includes('The Old Wing'))!.querySelector('.qt-badge-warning')!.textContent!.trim()).toBe('archived');
    expect(client.getQueryData(['scenarios', 'everywhere', { includeArchived: false }])).toBeDefined();
    expect(client.getQueryData(['scenarios', 'everywhere', { includeArchived: true }])).toBeDefined();
  });

  it('refetches on every open (staleTime: 0) rather than serving a cached shelf', async () => {
    const fixture = await render();
    const count = () => stub.sent.filter((r) => r.type === 'scenarioEverywhere').length;
    expect(count()).toBe(1);
    fixture.componentInstance.open.set(false);
    fixture.detectChanges();
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    await settle(fixture);
    expect(count()).toBe(2);
  });
});

describe('ScenarioBrowserDialog — highlight, preview and pick', () => {
  it('highlights on click; Use is disabled until something listed is highlighted', async () => {
    const fixture = await render();
    expect(button('Use this scenario').disabled).toBe(true);
    rows()[2].click();
    fixture.detectChanges();
    expect(rows()[2].getAttribute('aria-selected')).toBe('true');
    expect(rows()[0].getAttribute('aria-selected')).toBe('false');
    expect(button('Use this scenario').disabled).toBe(false);
  });

  it('counts a highlight the search filtered away as none', async () => {
    const fixture = await render();
    rows()[2].click();
    fixture.detectChanges();
    typeSearch(fixture, 'moor');
    expect(button('Use this scenario').disabled).toBe(true);
    expect(text()).not.toContain('Farhouse › The Attic');
    typeSearch(fixture, '');
    expect(button('Use this scenario').disabled).toBe(false);
  });

  it('previews the excerpt while the body loads, then the body as Markdown', async () => {
    let release!: () => void;
    const s = defaultStub();
    const real = s.read;
    s.read = (ref) => new Promise((r) => (release = () => r(real(ref))));
    const fixture = await render(s);
    rows()[2].click();
    fixture.detectChanges();
    await settle(fixture);
    expect(text()).toContain('Farhouse › The Attic');
    expect(text()).toContain('Dust, trunks and a single round window.');
    expect(stub.sent.filter((r) => r.type === 'scenarioReadByRef')).toEqual([
      { type: 'scenarioReadByRef', mountPointId: 'mp-far', path: 'Scenarios/the-attic.md' },
    ]);
    release();
    await settle(fixture);
    const preview = q('[data-testid="scenario-browser-preview"]')!;
    expect(preview.querySelector('h1')!.textContent).toBe('Body of Scenarios/the-attic.md');
    expect(preview.querySelector('em')!.textContent).toBe('full');
  });

  it('falls back to the excerpt when the body read fails', async () => {
    const s = defaultStub();
    s.read = async () => {
      throw new CoreDispatchError({ kind: 'notFound', message: 'Scenario not found' } as never);
    };
    const fixture = await render(s);
    rows()[2].click();
    fixture.detectChanges();
    await settle(fixture);
    expect(q('[data-testid="scenario-browser-preview"]')!.textContent!.trim()).toBe(
      'Dust, trunks and a single round window.',
    );
  });

  it('Use picks a store selection labelled Owner › Name for a shelf outside the chat, then closes', async () => {
    const fixture = await render();
    rows()[2].click();
    fixture.detectChanges();
    button('Use this scenario').click();
    fixture.detectChanges();
    expect(fixture.componentInstance.picks).toEqual([
      { kind: 'store', mountPointId: 'mp-far', path: 'Scenarios/the-attic.md', label: 'Farhouse › The Attic' },
    ]);
    expect(fixture.componentInstance.closes).toBe(1);
  });

  it('a double-click picks at once, snapping a pick inside the hierarchy native', async () => {
    const fixture = await render();
    rows()[0].dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    fixture.detectChanges();
    expect(fixture.componentInstance.picks).toEqual([{ kind: 'general', path: 'Scenarios/moor.md' }]);
    expect(fixture.componentInstance.closes).toBe(1);
  });

  it('ArrowDown / ArrowUp move the highlight, clamped at both ends; Enter uses it', async () => {
    const fixture = await render();
    const selected = () => rows().findIndex((r) => r.getAttribute('aria-selected') === 'true');
    expect(key(fixture, 'ArrowDown').defaultPrevented).toBe(true);
    expect(selected()).toBe(0);
    key(fixture, 'ArrowUp');
    expect(selected()).toBe(0);
    for (let i = 0; i < 6; i++) key(fixture, 'ArrowDown');
    expect(selected()).toBe(3);
    key(fixture, 'ArrowUp');
    expect(selected()).toBe(2);
    key(fixture, 'Enter');
    expect(fixture.componentInstance.picks).toEqual([
      { kind: 'store', mountPointId: 'mp-far', path: 'Scenarios/the-attic.md', label: 'Farhouse › The Attic' },
    ]);
  });

  it('moves from the LISTED highlight, not a stale one (read before set)', async () => {
    // L-H(2): the index is read from the highlighted row BEFORE the new
    // highlight is set. A highlight the search hid counts as none, so the
    // first ArrowDown lands on row 0 of the filtered list.
    const fixture = await render();
    rows()[3].click();
    fixture.detectChanges();
    typeSearch(fixture, 'the a');
    expect(rowNames()).toEqual(['The Atrium', 'The Attic']);
    key(fixture, 'ArrowDown');
    expect(rows()[0].getAttribute('aria-selected')).toBe('true');
    key(fixture, 'ArrowDown');
    expect(rows()[1].getAttribute('aria-selected')).toBe('true');
  });

  it('Enter with nothing highlighted does nothing', async () => {
    const fixture = await render();
    const event = key(fixture, 'Enter');
    expect(event.defaultPrevented).toBe(false);
    expect(fixture.componentInstance.picks).toEqual([]);
  });

  it('scrolls the newly highlighted row into view (block: nearest)', async () => {
    const fixture = await render();
    const seen: Array<{ row: string; opts: unknown }> = [];
    for (const r of rows()) {
      (r as unknown as { scrollIntoView: (o: unknown) => void }).scrollIntoView = (o) =>
        seen.push({ row: r.getAttribute('data-row')!, opts: o });
    }
    key(fixture, 'ArrowDown');
    expect(seen).toEqual([{ row: 'mp-gen|Scenarios/moor.md', opts: { block: 'nearest' } }]);
  });
});

describe('ScenarioBrowserDialog — Escape closes the browser only', () => {
  it('closes on Escape and stops it reaching a parent’s document listener', async () => {
    const fixture = await render();
    let parentSaw = 0;
    const parent = (e: KeyboardEvent) => {
      if (e.key === 'Escape') parentSaw += 1;
    };
    document.addEventListener('keydown', parent);
    try {
      q('input[type="search"]')!.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
      );
      fixture.detectChanges();
      expect(fixture.componentInstance.closes).toBe(1);
      expect(parentSaw).toBe(0);
      // Once the browser is gone, Escape is the parent's again.
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      expect(parentSaw).toBe(1);
      expect(fixture.componentInstance.closes).toBe(1);
    } finally {
      document.removeEventListener('keydown', parent);
    }
  });
});
