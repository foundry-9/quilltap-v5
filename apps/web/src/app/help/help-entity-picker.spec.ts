/**
 * `HelpEntityPicker` — asserted against v4
 * `components/help-chat/HelpEntityPicker.tsx` at `d883a5ee1`.
 *
 * The three `PARAM_ROUTES` rows are pinned against
 * `__fixtures__/param-routes-vectors.json` — a probe of v4's REAL table through
 * its exported `hasParamSegments` / `findParamRoute`, since the table itself is
 * module-private. That corpus is what catches the edges: the regexes are
 * case-sensitive and anchored, so `/AURORA/:id`, `aurora/:id` (no leading
 * slash) and `/aurora/:id2` all miss, and each row has its OWN empty-label
 * fallback ("Unnamed" / "Untitled chat" / "Unnamed project").
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { HelpEntityPicker, findParamRoute, hasParamSegments } from './help-entity-picker';
import vectors from './__fixtures__/param-routes-vectors.json';

describe('PARAM_ROUTES — the recorded-vector corpus (from v4 itself)', () => {
  for (const row of vectors.paramRoutes) {
    it(`${JSON.stringify(row.url)} → ${row.entityLabel ?? 'no route'}`, () => {
      expect(hasParamSegments(row.url)).toBe(row.hasParamSegments);
      const route = findParamRoute(row.url);
      if (!row.entityLabel) {
        expect(route).toBeNull();
        return;
      }
      expect(route).not.toBeNull();
      expect(route!.entityLabel).toBe(row.entityLabel);
      expect(route!.apiUrl).toBe(row.apiUrl);
      expect(route!.pattern.source).toBe(row.pattern);
      expect(route!.buildUrl(row.url, 'ID-42')).toBe(row.builtUrl);
      expect(route!.getLabel({ name: 'Nom', title: 'Titre' })).toBe(row.labelNamed);
      expect(route!.getLabel({})).toBe(row.labelEmpty);
      // A blank name falls back too — v4 uses `||`, not `??`.
      expect(route!.getLabel({ name: '', title: '' })).toBe(row.labelBlank);
      expect(route!.getId({ id: 'x-1' })).toBe(row.id);
    });
  }
});

// ---------------------------------------------------------------------------
// The picker dialog.
// ---------------------------------------------------------------------------

let dispatchData: ReturnType<typeof vi.fn>;

function stubCore(reply: unknown, fail = false) {
  dispatchData = vi.fn(async () => {
    if (fail) throw new Error('Failed to load');
    return reply as Record<string, unknown>;
  });
  return {
    events$: new Subject().asObservable(),
    dispatchData,
  } as unknown as CoreClient;
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  urlTemplate: string,
  reply: unknown,
  fail = false,
): Promise<ComponentFixture<HelpEntityPicker>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [HelpEntityPicker],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: stubCore(reply, fail) },
    ],
  });
  const fixture = TestBed.createComponent(HelpEntityPicker);
  fixture.componentRef.setInput('urlTemplate', urlTemplate);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function items(fixture: ComponentFixture<unknown>): string[] {
  return Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-help-entity-picker-item'),
  ).map((el) => el.textContent?.trim() ?? '');
}

afterEach(() => TestBed.resetTestingModule());

describe('HelpEntityPicker — the dialog', () => {
  it('lists characters from the keyed reply', async () => {
    const fixture = await render('/aurora/:id/edit', {
      characters: [
        { id: 'c1', name: 'Jeeves' },
        { id: 'c2', name: '' },
      ],
    });
    expect(dispatchData).toHaveBeenCalledWith({ type: 'characterList' });
    expect(items(fixture)).toEqual(['Jeeves', 'Unnamed']);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Select a Character');
  });

  it('lists chats from the BARE array reply', async () => {
    // v5's `listChats` answers the array as its whole `data`, where
    // `characterList` and `projectList` answer a keyed object — the one place
    // v4's single `responseKey` does not carry over.
    const fixture = await render('/salon/:id', [
      { id: 's1', title: 'A chat' },
      { id: 's2', title: null },
    ]);
    expect(dispatchData).toHaveBeenCalledWith({ type: 'listChats' });
    expect(items(fixture)).toEqual(['A chat', 'Untitled chat']);
  });

  it('lists projects from the keyed reply', async () => {
    const fixture = await render('/prospero/:id/files', { projects: [{ id: 'p1', name: 'Opus' }] });
    expect(dispatchData).toHaveBeenCalledWith({ type: 'projectList' });
    expect(items(fixture)).toEqual(['Opus']);
  });

  it('resolves the :id and emits the built URL', async () => {
    const fixture = await render('/aurora/:id/edit', { characters: [{ id: 'c1', name: 'Jeeves' }] });
    const emitted: string[] = [];
    fixture.componentInstance.selectEntity.subscribe((u) => emitted.push(u));
    (
      (fixture.nativeElement as HTMLElement).querySelector(
        '.qt-help-entity-picker-item',
      ) as HTMLElement
    ).click();
    expect(emitted).toEqual(['/aurora/c1/edit']);
  });

  it('refuses an unknown entity type without dispatching anything', async () => {
    const fixture = await render('/settings/:id', {});
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Unknown entity type');
    expect(dispatchData).not.toHaveBeenCalled();
  });

  it('surfaces a load failure', async () => {
    const fixture = await render('/aurora/:id', {}, true);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Failed to load');
  });

  it('shows the per-entity empty state', async () => {
    const fixture = await render('/prospero/:id', { projects: [] });
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('No projects found');
  });

  it('offers a filter only above five items, and filters on it', async () => {
    const five = await render('/aurora/:id', {
      characters: Array.from({ length: 5 }, (_, i) => ({ id: 'c' + i, name: 'Name' + i })),
    });
    expect(
      (five.nativeElement as HTMLElement).querySelector('.qt-help-entity-picker-filter'),
    ).toBeNull();

    const six = await render('/aurora/:id', {
      characters: [
        ...Array.from({ length: 5 }, (_, i) => ({ id: 'c' + i, name: 'Name' + i })),
        { id: 'z', name: 'Zeppelin' },
      ],
    });
    const input = (six.nativeElement as HTMLElement).querySelector(
      '.qt-help-entity-picker-filter',
    ) as HTMLInputElement;
    expect(input).not.toBeNull();
    input.value = 'zepp';
    input.dispatchEvent(new Event('input'));
    six.detectChanges();
    expect(items(six)).toEqual(['Zeppelin']);
    // v4's "No matches" is a DIFFERENT sentence from the empty-list one.
    input.value = 'nothing-here';
    input.dispatchEvent(new Event('input'));
    six.detectChanges();
    expect((six.nativeElement as HTMLElement).textContent).toContain('No matches');
  });

  it('cancels on the backdrop and on the close button', async () => {
    const fixture = await render('/aurora/:id', { characters: [] });
    let cancels = 0;
    fixture.componentInstance.cancel.subscribe(() => (cancels += 1));
    (
      (fixture.nativeElement as HTMLElement).querySelector(
        '.qt-help-entity-picker-backdrop',
      ) as HTMLElement
    ).click();
    (
      (fixture.nativeElement as HTMLElement).querySelector(
        '.qt-help-entity-picker-header button',
      ) as HTMLElement
    ).click();
    expect(cancels).toBe(2);
  });

  it('does not cancel on a click inside the panel', async () => {
    const fixture = await render('/aurora/:id', { characters: [] });
    let cancels = 0;
    fixture.componentInstance.cancel.subscribe(() => (cancels += 1));
    (
      (fixture.nativeElement as HTMLElement).querySelector('.qt-help-entity-picker') as HTMLElement
    ).click();
    expect(cancels).toBe(0);
  });

  it('survives a reply of the wrong shape', async () => {
    const fixture = await render('/aurora/:id', { characters: 'not an array' });
    expect(items(fixture)).toEqual([]);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('No characters found');
  });
});
