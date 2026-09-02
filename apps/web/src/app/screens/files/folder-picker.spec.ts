import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { FolderPicker } from './folder-picker';

/**
 * Parity spec for the folder dropdown in Move to Project — a 1:1 transcription of
 * v4's `__tests__/unit/components/files/FolderPicker.test.tsx` at `a00e18f0d`
 * (the P4.D132 / P4.D86 client-parity precedent).
 *
 * v4's own header: bug 113's derived folder list was mirrored into component
 * state behind an "only if empty" guard. Root is seeded unconditionally, before
 * any data is consulted, so the still-loading first render produced a one-entry
 * list that satisfied the guard — the mirror was filled with the loading state
 * and sealed against every update that followed, including a change of
 * destination. What's worth pinning is therefore not the rendering but the
 * *re-derivation*: folders must appear once the queries settle, and must change
 * when `projectId` does.
 *
 * v5 never carried the latch — it had no folder picker at all (a free-text path
 * field stood in behind the P4.6af tier-3 deferral), so this lane builds the
 * component in v4's post-fix shape rather than porting a diff. The specs pin the
 * shape either way.
 */

interface Stub {
  files?: { folderPath?: string }[];
  folders?: { id: string; path: string; name: string }[];
}

/** Stub both dispatches the picker reads, keyed by the project in the request. */
function stubRoutes(byProject: Record<string, Stub>): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; projectId?: string | null; filter?: string }) => {
      const key = req.projectId ?? 'general';
      const stub = byProject[key] ?? {};
      if (req.type === 'filesFoldersList') {
        return { folders: stub.folders ?? [] };
      }
      return { files: stub.files ?? [] };
    }) as CoreClient['dispatchData'],
  };
}

async function render(
  client: Partial<CoreClient>,
  projectId: string,
): Promise<ComponentFixture<FolderPicker>> {
  TestBed.configureTestingModule({
    imports: [FolderPicker],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(FolderPicker);
  fixture.componentRef.setInput('value', '/');
  fixture.componentRef.setInput('projectId', projectId);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

/** v4's `waitFor` twin: let the queries resolve, then re-render. */
async function settle(fixture: ComponentFixture<FolderPicker>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function options(fixture: ComponentFixture<FolderPicker>): HTMLOptionElement[] {
  return Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLOptionElement>('select option'),
  );
}

/** Option labels, trimmed, with the trailing " (N files)" count stripped. */
function optionLabels(fixture: ComponentFixture<FolderPicker>): string[] {
  return options(fixture).map((o) => (o.textContent ?? '').trim().replace(/ \(\d+ files\)$/, ''));
}

describe('FolderPicker', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('lists the project folders once the queries settle (bug 113)', async () => {
    const fixture = await render(
      stubRoutes({
        estate: {
          files: [{ folderPath: '/Gary/' }],
          folders: [
            { id: 'f1', path: '/Gary/', name: 'Gary' },
            { id: 'f2', path: '/character-avatars/', name: 'character-avatars' },
          ],
        },
      }),
      'estate',
    );

    expect(optionLabels(fixture)).toEqual(
      expect.arrayContaining(['└ Gary', '└ character-avatars']),
    );
    // The bug's exact signature: Root, alone, forever.
    expect(optionLabels(fixture)).not.toEqual(['/ (Root)']);
  });

  it('re-derives when the destination project changes (bug 113)', async () => {
    const fixture = await render(
      stubRoutes({
        estate: { folders: [{ id: 'f1', path: '/Gary/', name: 'Gary' }] },
        plans: { folders: [{ id: 'f2', path: '/Scenarios/', name: 'Scenarios' }] },
      }),
      'estate',
    );
    expect(optionLabels(fixture)).toContain('└ Gary');

    fixture.componentRef.setInput('projectId', 'plans');
    fixture.detectChanges();
    await settle(fixture);

    expect(optionLabels(fixture)).toContain('└ Scenarios');
    expect(optionLabels(fixture)).not.toContain('└ Gary');
  });

  it('offers Root alone for a project that genuinely has no folders', async () => {
    const fixture = await render(stubRoutes({ empty: { files: [], folders: [] } }), 'empty');

    expect(optionLabels(fixture)).toEqual(['/ (Root)']);
  });

  it('indents a nested folder beneath its parent', async () => {
    const fixture = await render(
      stubRoutes({
        plans: {
          folders: [
            { id: 'f1', path: '/Foundry-9/', name: 'Foundry-9' },
            { id: 'f2', path: '/Foundry-9/Quilltap/', name: 'Quilltap' },
          ],
        },
      }),
      'plans',
    );

    expect(optionLabels(fixture)).toContain('└ Foundry-9');
    const nested = options(fixture).find((o) => (o.textContent ?? '').includes('Quilltap'));
    // Non-breaking spaces, because an <option> collapses ordinary ones. Asserted
    // as explicit escapes: v4's source hides them as look-alike spaces.
    expect(nested?.textContent).toContain('  └ Quilltap');
  });
});

/**
 * The create-folder affordance (v4 `:160-206`). Both arms: a successful create
 * REFETCHES the folder list (copying the previous render's snapshot could not
 * have contained the folder just created), and a failed one falls back to a
 * project-scoped local entry while still moving the selection.
 */
describe('FolderPicker create-folder', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  function openCreateInput(fixture: ComponentFixture<FolderPicker>): void {
    const plus = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
      (b) => b.getAttribute('title') === 'Create new folder',
    )!;
    plus.click();
    fixture.detectChanges();
  }

  function typeAndCreate(fixture: ComponentFixture<FolderPicker>, path: string): void {
    const input = (fixture.nativeElement as HTMLElement).querySelector('input')!;
    input.value = path;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    const create = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Create')!;
    create.click();
  }

  it('refetches after a successful create, so the new folder is offered', async () => {
    // The stub only reports the folder AFTER the create call — a snapshot copy
    // could not contain it, so only a real refetch can put it in the list.
    let created = false;
    const emitted: string[] = [];
    const client: Partial<CoreClient> = {
      dispatchData: (async (req: { type: string; path?: string }) => {
        if (req.type === 'filesFolderCreate') {
          created = true;
          return { folder: { id: 'new', path: req.path, name: 'Ledgers' }, alreadyExists: false };
        }
        if (req.type === 'filesFoldersList') {
          return {
            folders: created ? [{ id: 'new', path: '/Ledgers/', name: 'Ledgers' }] : [],
          };
        }
        return { files: [] };
      }) as CoreClient['dispatchData'],
    };

    const fixture = await render(client, 'estate');
    fixture.componentInstance.valueChange.subscribe((p: string) => emitted.push(p));
    expect(optionLabels(fixture)).toEqual(['/ (Root)']);

    openCreateInput(fixture);
    typeAndCreate(fixture, 'Ledgers');
    await settle(fixture);

    expect(optionLabels(fixture)).toContain('\u2514 Ledgers');
    expect(emitted).toEqual(['/Ledgers/']);
    // The input closes and clears on success.
    expect((fixture.nativeElement as HTMLElement).querySelector('input')).toBeNull();
  });

  it('normalizes a bare name to /name/ before creating', async () => {
    const paths: string[] = [];
    const client: Partial<CoreClient> = {
      dispatchData: (async (req: { type: string; path?: string }) => {
        if (req.type === 'filesFolderCreate') {
          paths.push(req.path!);
          return { folder: { path: req.path } };
        }
        if (req.type === 'filesFoldersList') return { folders: [] };
        return { files: [] };
      }) as CoreClient['dispatchData'],
    };
    const fixture = await render(client, 'estate');
    openCreateInput(fixture);
    typeAndCreate(fixture, '  Ledgers  ');
    await settle(fixture);
    expect(paths).toEqual(['/Ledgers/']);
  });

  it('falls back to a project-scoped local entry when the create fails', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const emitted: string[] = [];
    const client: Partial<CoreClient> = {
      dispatchData: (async (req: { type: string }) => {
        if (req.type === 'filesFolderCreate') throw new Error('the registry is unreachable');
        if (req.type === 'filesFoldersList') return { folders: [] };
        return { files: [] };
      }) as CoreClient['dispatchData'],
    };

    const fixture = await render(client, 'estate');
    fixture.componentInstance.valueChange.subscribe((p: string) => emitted.push(p));

    openCreateInput(fixture);
    typeAndCreate(fixture, '/Ledgers/');
    await settle(fixture);

    expect(optionLabels(fixture)).toContain('\u2514 Ledgers');
    expect(emitted).toEqual(['/Ledgers/']);

    // Scoped to its project: a destination change drops it rather than offering
    // a folder that belongs to somewhere else.
    fixture.componentRef.setInput('projectId', 'plans');
    fixture.detectChanges();
    await settle(fixture);
    expect(optionLabels(fixture)).toEqual(['/ (Root)']);
  });
});

/**
 * BEYOND v4's CORPUS — the exact option bytes.
 *
 * v4's nested-indent case asserts `textContent` *contains*
 * `\u00a0\u00a0\u2514 Quilltap`, which a WIDER indent satisfies too: dropping
 * v4's `depth - 1` (so every level gains a step) leaves all four of its cases
 * green. The rule under test is `'\u00a0\u00a0'.repeat(depth - 1)`, so it is
 * pinned here by whole-label equality over a three-deep tree.
 */
describe('FolderPicker option labels', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders each label byte-for-byte, two NBSPs per level below the first', async () => {
    const fixture = await render(
      stubRoutes({
        deep: {
          files: [{ folderPath: '/Attic/Crates/Ledgers/' }, { folderPath: '/' }],
          folders: [
            { id: 'f1', path: '/Attic/', name: 'Attic' },
            { id: 'f2', path: '/Attic/Crates/', name: 'Crates' },
            { id: 'f3', path: '/Attic/Crates/Ledgers/', name: 'Ledgers' },
          ],
        },
      }),
      'deep',
    );

    expect(options(fixture).map((o) => o.textContent)).toEqual([
      '/ (Root) (1 files)',
      '\u2514 Attic',
      '\u00a0\u00a0\u2514 Crates',
      '\u00a0\u00a0\u00a0\u00a0\u2514 Ledgers (1 files)',
    ]);
  });
});
