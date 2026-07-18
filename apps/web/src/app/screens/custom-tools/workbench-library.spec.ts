import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { describe, expect, it, afterEach } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CustomToolLibraryEntry, CustomToolLibraryError } from '../../core/core-contract';
import { MAX_ROSTER_SIZE } from '../../pascal/custom-tool-types';
import { WorkbenchLibrary } from './workbench-library';

/**
 * WorkbenchLibrary — asserted against v4 `WorkbenchLibrary.tsx`'s exact
 * behaviors: the advisory thresholds and their verbatim copy, the per-kind badge
 * variants, broken files LISTED (never hidden) with the loader's reason
 * verbatim, the group-by-store toggle, and the duplicate flow re-READING the
 * file rather than reconstructing it from the summary entry.
 */

function tool(over: Partial<CustomToolLibraryEntry> = {}): CustomToolLibraryEntry {
  return {
    valid: true,
    name: 'unlock',
    title: 'Unlock',
    description: 'Pick the lock.',
    disabled: false,
    defaultVisibility: 'public',
    rollForm: 'range',
    llm: false,
    parameterCount: 0,
    outcomeCount: 2,
    mountPointId: 'm1',
    mountName: 'General',
    definitionPath: 'Tools/unlock.tool.json',
    attachments: [{ kind: 'general', label: 'The General Store' }],
    ...over,
  };
}

function brokenEntry(over: Partial<CustomToolLibraryError> = {}): CustomToolLibraryError {
  return {
    valid: false,
    definitionPath: 'Tools/broken.tool.json',
    mountPointId: 'm1',
    mountName: 'General',
    reason: 'outcomes.0.when: must test something',
    attachments: [{ kind: 'general', label: 'The General Store' }],
    ...over,
  };
}

interface Req {
  type: string;
  [k: string]: unknown;
}

function stubClient(
  tools: CustomToolLibraryEntry[],
  errors: CustomToolLibraryError[] = [],
  hooks: { onDispatch?: (req: Req) => void; fileContent?: string } = {},
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      hooks.onDispatch?.(req);
      if (req.type === 'customToolsLibrary') return { tools, errors };
      if (req.type === 'mountFileRead') return { content: hooks.fileContent ?? '{}' };
      if (req.type === 'mountFileDelete') return {};
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<WorkbenchLibrary>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WorkbenchLibrary],
    providers: [provideRouter([]), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(WorkbenchLibrary);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

describe('WorkbenchLibrary (v4 WorkbenchLibrary.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('lists the tools the library GET returned', async () => {
    const fixture = await render(stubClient([tool(), tool({ name: 'burn', title: 'Burn' })]));
    expect(text(fixture)).toContain('Unlock');
    expect(text(fixture)).toContain('Burn');
  });

  it('shows the bare-baize empty state only when there are no tools AND no errors', async () => {
    const empty = await render(stubClient([], []));
    expect(text(empty)).toContain('The baize is bare');

    const withBroken = await render(stubClient([], [brokenEntry()]));
    expect(text(withBroken)).not.toContain('The baize is bare');
  });

  it('LISTS broken files rather than hiding them, with the loader reason verbatim', async () => {
    const fixture = await render(
      stubClient([], [brokenEntry({ reason: 'roll: must be dice notation like "3d6+2"' })]),
    );
    const body = text(fixture);
    expect(body).toContain('Cards that would not read');
    expect(body).toContain('Tools/broken.tool.json');
    // Verbatim — the browser never re-derives the loader's sentence.
    expect(body).toContain('roll: must be dice notation like "3d6+2"');
    expect(body).toContain('will not read');
  });

  it('flags a name defined in more than one store, carrying v4 tier advisory copy', async () => {
    const fixture = await render(
      stubClient([
        tool({ mountPointId: 'm1', mountName: 'General' }),
        tool({ mountPointId: 'm2', mountName: "Aria's vault" }),
      ]),
    );
    expect(text(fixture)).toContain('this name is defined in 2 places');

    const advisory = (fixture.nativeElement as HTMLElement).querySelector(
      '[title^="Nearest tier wins"]',
    );
    expect(advisory?.getAttribute('title')).toBe(
      'Nearest tier wins: character → participant → group → project → global. ' +
        'A disabled definition at a nearer tier suppresses the name outward. ' +
        'Which one a given chat deals depends on who is rolling.',
    );
  });

  it('does not flag a name that lives in exactly one store', async () => {
    const fixture = await render(stubClient([tool()]));
    expect(text(fixture)).not.toContain('this name is defined in');
  });

  it('warns only when a store passes MAX_ROSTER_SIZE, not at it', async () => {
    const atCap = Array.from({ length: MAX_ROSTER_SIZE }, (_, i) => tool({ name: `t${i}` }));
    const at = await render(stubClient(atCap));
    expect(text(at)).not.toContain('more than the');

    const over = await render(stubClient([...atCap, tool({ name: 'one-more' })]));
    expect(text(over)).toContain(`more than the ${MAX_ROSTER_SIZE} a single chat`);
  });

  it('renders a distinct badge variant per attachment kind', async () => {
    const fixture = await render(
      stubClient([
        tool({
          attachments: [
            { kind: 'general', label: 'The General Store' },
            { kind: 'character', id: 'c1', label: 'Aria' },
            { kind: 'group', id: 'g1', label: 'The Salon' },
            { kind: 'project', id: 'p1', label: 'Ledger' },
            { kind: 'unattached', label: 'Unattached' },
          ],
        }),
      ]),
    );
    const body = text(fixture);
    expect(body).toContain('The General Store');
    expect(body).toContain("Aria's vault");
    expect(body).toContain('Group: The Salon');
    expect(body).toContain('Project: Ledger');
    expect(body).toContain('Unattached');

    const html = (fixture.nativeElement as HTMLElement).innerHTML;
    expect(html).toContain('qt-badge qt-badge-primary');
    expect(html).toContain('qt-badge qt-badge-info');
    expect(html).toContain('qt-badge qt-badge-secondary');
    expect(html).toContain('qt-badge qt-badge-success');
  });

  it('badges disabled and whisper defaults, and the roll form', async () => {
    const fixture = await render(
      stubClient([tool({ disabled: true, defaultVisibility: 'whisper', rollForm: 'dice' })]),
    );
    const body = text(fixture);
    expect(body).toContain('disabled');
    expect(body).toContain('whisper');
    expect(body).toContain('dice');
  });

  it('singularizes the parameter and outcome counts the way v4 does', async () => {
    const one = await render(stubClient([tool({ parameterCount: 1, outcomeCount: 1 })]));
    expect(text(one)).toContain('1 param');
    expect(text(one)).not.toContain('1 params');
    expect(text(one)).toContain('1 outcome');

    const many = await render(stubClient([tool({ parameterCount: 3, outcomeCount: 4 })]));
    expect(text(many)).toContain('3 params');
    expect(text(many)).toContain('4 outcomes');
  });

  it('omits the parameter badge entirely at zero (v4 renders it only when > 0)', async () => {
    const fixture = await render(stubClient([tool({ parameterCount: 0 })]));
    expect(text(fixture)).not.toContain('0 param');
  });

  it('groups by store when toggled, and sorts the groups by store name', async () => {
    const fixture = await render(
      stubClient([
        tool({ name: 'z', title: 'Z', mountPointId: 'm2', mountName: 'Zephyr' }),
        tool({ name: 'a', title: 'A', mountPointId: 'm1', mountName: 'Apex' }),
      ]),
    );
    expect(fixture.componentInstance.groups()).toBeNull();

    fixture.componentInstance.groupByStore.set(true);
    fixture.detectChanges();
    const groups = fixture.componentInstance.groups();
    expect(groups?.map((g) => g.mountName)).toEqual(['Apex', 'Zephyr']);
  });

  it('filters on name, title, and description', async () => {
    const fixture = await render(
      stubClient([
        tool({ name: 'unlock', title: 'Unlock', description: 'Pick the lock.' }),
        tool({ name: 'scry', title: 'Scry', description: 'Peer into the aether.' }),
      ]),
    );
    const names = () => fixture.componentInstance.filteredTools().map((t) => t.name);

    fixture.componentInstance.search.set('aether');
    expect(names()).toEqual(['scry']);

    fixture.componentInstance.search.set('UNLOCK');
    expect(names()).toEqual(['unlock']);

    fixture.componentInstance.search.set('');
    expect(names().sort()).toEqual(['scry', 'unlock']);
  });

  it('sorts tools by title, then by store name', async () => {
    const fixture = await render(
      stubClient([
        tool({ name: 'b', title: 'Beta', mountName: 'Zephyr' }),
        tool({ name: 'a2', title: 'Alpha', mountName: 'Zephyr' }),
        tool({ name: 'a1', title: 'Alpha', mountName: 'Apex' }),
      ]),
    );
    expect(fixture.componentInstance.filteredTools().map((t) => t.name)).toEqual([
      'a1',
      'a2',
      'b',
    ]);
  });

  it('duplicate RE-READS the file and emits its bytes as the template', async () => {
    const seen: Req[] = [];
    const bytes = '{\n  "name": "unlock"\n}\n';
    const fixture = await render(
      stubClient([tool()], [], { onDispatch: (r) => seen.push(r), fileContent: bytes }),
    );

    let emitted: string | undefined;
    fixture.componentInstance.duplicateTemplate.subscribe((t) => (emitted = t));
    await fixture.componentInstance.duplicate(tool());

    // The library entry is a SUMMARY (counts, not the outcome table), so only
    // the bytes on disk can seed a faithful copy.
    expect(seen.some((r) => r.type === 'mountFileRead')).toBe(true);
    expect(emitted).toBe(bytes);
  });

  it('opening a broken entry drills to the same editor the valid rows use', async () => {
    const fixture = await render(stubClient([], [brokenEntry()]));
    let opened: { mountPointId: string; path: string } | undefined;
    fixture.componentInstance.open.subscribe((e) => (opened = e));

    const repair = [
      ...(fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ].find((b) => b.textContent?.trim() === 'Repair');
    repair?.dispatchEvent(new Event('click'));

    expect(opened).toEqual({ mountPointId: 'm1', path: 'Tools/broken.tool.json' });
  });

  it('surfaces a library read failure', async () => {
    const fixture = await render({
      dispatchData: (async () => {
        throw new Error('the ledger is shut');
      }) as CoreClient['dispatchData'],
    });
    expect(text(fixture)).toContain('the ledger is shut');
  });
});
