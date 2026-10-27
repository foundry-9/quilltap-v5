import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, provideRouter, type ParamMap } from '@angular/router';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { CoreDispatchError } from '../../core/core-contract';
import { emptyDraftInventoryGesture } from '../../pascal/tool-draft';
import { CustomToolsPage } from './custom-tools-page';
import { DestinationPicker } from './destination-picker';
import { WorkbenchEditor } from './workbench-editor';
import { ToastService } from '../../ui/toast.service';

/**
 * WorkbenchEditor + DestinationPicker + the route shell — asserted against v4's
 * exact behaviors. The load-bearing claims:
 *
 *   - a valid file opens in FORM mode; an invalid or unparseable one opens in
 *     JSON mode with the repair banner and the loader's reason verbatim;
 *   - JSON mode writes the user's bytes VERBATIM (canonicalization is
 *     form-mode-only, §6.2);
 *   - `saveIsBlocked`'s truth table, including repair mode's deliberate
 *     exception;
 *   - a same-location save carries the held mtime; a save-as carries none;
 *   - the conflict flow, keyed off the TYPED error kind rather than a message.
 */

const VALID_DEFINITION = JSON.stringify(
  {
    $schema: '/schemas/qtap-custom-tool.schema.json',
    name: 'unlock',
    description: 'Pick the lock.',
    outcomes: [
      { when: { gte: 0.5 }, message: 'Open.', state: 'success' },
      { when: true, message: 'Stuck.', state: 'failure' },
    ],
  },
  null,
  2,
);

interface Req {
  type: string;
  [k: string]: unknown;
}

function conflictError(): CoreDispatchError {
  return new CoreDispatchError({
    kind: 'conflict',
    message: 'Document was modified by another process (mtime mismatch).',
  });
}

function notFoundError(): CoreDispatchError {
  return new CoreDispatchError({ kind: 'not-found', message: 'No such file' });
}

function stubClient(
  hooks: {
    onDispatch?: (req: Req) => void;
    content?: string;
    mtime?: number;
    readThrows?: () => unknown;
    writeThrows?: () => unknown;
    destinations?: unknown;
  } = {},
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      hooks.onDispatch?.(req);
      if (req.type === 'mountFileRead') {
        if (hooks.readThrows) throw hooks.readThrows();
        return { content: hooks.content ?? VALID_DEFINITION, mtime: hooks.mtime ?? 1000 };
      }
      if (req.type === 'mountFileWrite') {
        if (hooks.writeThrows) throw hooks.writeThrows();
        return { mtime: 2000 };
      }
      if (req.type === 'customToolsDestinations') {
        return (
          hooks.destinations ?? {
            general: { mountPointId: 'm1', mountName: 'General', existingToolNames: [] },
            projects: [],
            groups: [],
            characters: [],
            other: [],
          }
        );
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function renderEditor(
  client: Partial<CoreClient>,
  inputs: { source?: { mountPointId: string; path: string }; create?: unknown } = {},
): Promise<ComponentFixture<WorkbenchEditor>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WorkbenchEditor],
    providers: [provideRouter([]), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(WorkbenchEditor);
  if (inputs.source) fixture.componentRef.setInput('source', inputs.source);
  if (inputs.create) fixture.componentRef.setInput('create', inputs.create);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

const SOURCE = { mountPointId: 'm1', path: 'Tools/unlock.tool.json' };

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('WorkbenchEditor (v4 WorkbenchEditor.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('loads a valid definition into FORM mode via mountFileRead', async () => {
    const seen: Req[] = [];
    const fixture = await renderEditor(stubClient({ onDispatch: (r) => seen.push(r) }), {
      source: SOURCE,
    });
    const editor = fixture.componentInstance;

    // §W2: the dispatch verb, never the refusal-armed REST JSON leg. (Found by
    // type: since P4.D320 the editor also reads the destinations for its
    // wardrobe vantage, and that read may land first.)
    expect(seen.find((r) => r.type === 'mountFileRead')).toMatchObject({
      type: 'mountFileRead',
      ...SOURCE,
    });
    expect(editor.editorMode()).toBe('form');
    expect(editor.repairReason()).toBeNull();
    expect(editor.draft()?.name).toBe('unlock');
    expect(editor.loadedName()).toBe('unlock');
    expect(editor.mtime()).toBe(1000);
    expect(editor.dirty()).toBe(false);
  });

  it('opens an UNPARSEABLE file in JSON repair mode, bytes intact', async () => {
    const fixture = await renderEditor(stubClient({ content: '{ not json' }), { source: SOURCE });
    const editor = fixture.componentInstance;

    expect(editor.editorMode()).toBe('json');
    expect(editor.repairReason()).toContain('This file is not valid JSON');
    expect(editor.jsonText()).toBe('{ not json');
    expect(editor.draft()).toBeNull();
    expect(text(fixture)).toContain('Repair mode.');
  });

  it('opens a SCHEMA-INVALID file in repair mode with the loader sentence verbatim', async () => {
    const broken = JSON.stringify({ name: 'broken', description: 'x', outcomes: [] });
    const fixture = await renderEditor(stubClient({ content: broken }), { source: SOURCE });
    const editor = fixture.componentInstance;

    expect(editor.editorMode()).toBe('json');
    // The same sentence the server would have produced — not a re-derived one.
    expect(editor.repairReason()).toBe('outcomes: at least one outcome is required');
  });

  it('surfaces a load failure with a way back to the library', async () => {
    const fixture = await renderEditor(
      stubClient({
        readThrows: () => new Error('the store is shut'),
      }),
      { source: SOURCE },
    );
    expect(text(fixture)).toContain('the store is shut');
    expect(text(fixture)).toContain('Back to the library');
  });

  it('starts a bare create in form mode with a fresh draft', async () => {
    const fixture = await renderEditor(stubClient(), { create: {} });
    expect(fixture.componentInstance.editorMode()).toBe('form');
    expect(fixture.componentInstance.draft()?.name).toBe('');
    expect(fixture.componentInstance.location()).toBeNull();
  });

  it('two gestures inside one render window build on each other (v4 React flushes a discrete event)', async () => {
    // Measured LIVE (`workbench-grammar-flow` (h)/(i)): a second gesture that
    // lands before the zoneless scheduler's pass read the section's STALE
    // `draft` input — the second "Add effect" replaced the first, and an edit
    // straight after a second "add condition" dropped a chip. No
    // `detectChanges()` between the gestures here, on purpose.
    const fixture = await renderEditor(stubClient(), { create: {} });
    const root = fixture.nativeElement as HTMLElement;
    const button = (label: RegExp): HTMLButtonElement => {
      const found = [...root.querySelectorAll('button')].find((b) => label.test(b.textContent ?? ''));
      if (!found) throw new Error(`no button ${label}`);
      return found;
    };
    button(/Add effect/).click();
    button(/Add effect/).click();
    expect(fixture.componentInstance.draft()?.effects).toHaveLength(2);

    const gate = root.querySelector('qt-gate-section') as HTMLElement;
    const only = [...gate.querySelectorAll('[role="radio"]')].find((b) =>
      (b.textContent ?? '').includes('Only show if'),
    ) as HTMLElement;
    only.click();
    const addCondition = (): void =>
      ([...gate.querySelectorAll('button')].find((b) => (b.textContent ?? '').includes('add condition')) as HTMLButtonElement).click();
    addCondition();
    addCondition();
    // The new chip is in the DOM before the gesture returns, and an edit to it
    // keeps its sibling.
    const keys = gate.querySelectorAll<HTMLInputElement>('input[aria-label="Metadata key"]');
    expect(keys).toHaveLength(2);
    keys[1].value = 'b';
    keys[1].dispatchEvent(new Event('input'));
    expect(fixture.componentInstance.draft()?.gateConditions.map((c) => c.key)).toEqual(['', 'b']);
  });

  it('a duplicate template pre-fills but keeps NO location — it must save-as', async () => {
    const fixture = await renderEditor(stubClient(), {
      create: { template: VALID_DEFINITION },
    });
    expect(fixture.componentInstance.draft()?.name).toBe('unlock');
    // A copy has nowhere to go back to; Save must route through the picker.
    expect(fixture.componentInstance.location()).toBeNull();
    expect(fixture.componentInstance.loadedName()).toBeNull();
  });

  it('JSON mode writes the user bytes VERBATIM; form mode canonicalizes', async () => {
    const fixture = await renderEditor(stubClient(), { source: SOURCE });
    const editor = fixture.componentInstance;

    // Form mode emits the canonical serialization.
    expect(editor.contentToWrite()).toBe(
      `${JSON.stringify(JSON.parse(VALID_DEFINITION), null, 2)}\n`,
    );

    // JSON mode hands back exactly what is in the box — odd spacing included.
    editor.switchToJson();
    const scruffy = '{"name":"unlock",   "description":"x"}';
    editor.jsonText.set(scruffy);
    expect(editor.contentToWrite()).toBe(scruffy);
  });

  it('the saveIsBlocked truth table matches v4', async () => {
    const fixture = await renderEditor(stubClient(), { source: SOURCE });
    const editor = fixture.componentInstance;

    // Form mode with a valid draft: not blocked.
    expect(editor.saveIsBlocked()).toBe(false);

    // Form mode with a broken draft: blocked.
    editor.onDraftChange({ ...editor.draft()!, name: '' });
    expect(editor.saveIsBlocked()).toBe(true);
  });

  it('repair mode is the ONE exception — an invalid file may be saved broken', async () => {
    const fixture = await renderEditor(stubClient({ content: '{ not json' }), { source: SOURCE });
    const editor = fixture.componentInstance;
    await settle(fixture);

    // JSON invalid, but repairReason is set: save is NOT blocked.
    expect(editor.repairReason()).not.toBeNull();
    expect(editor.saveIsBlocked()).toBe(false);
  });

  it('a same-location save carries the held mtime; a save-as carries none', async () => {
    const seen: Req[] = [];
    const fixture = await renderEditor(stubClient({ onDispatch: (r) => seen.push(r) }), {
      source: SOURCE,
    });
    await fixture.componentInstance.save(SOURCE);
    const sameFile = seen.find((r) => r.type === 'mountFileWrite');
    expect(sameFile?.['expectedMtime']).toBe(1000);

    seen.length = 0;
    await fixture.componentInstance.save({ mountPointId: 'm2', path: 'Tools/copy.tool.json' });
    const elsewhere = seen.find((r) => r.type === 'mountFileWrite');
    expect(elsewhere?.['expectedMtime']).toBeUndefined();
  });

  it('a successful save updates location, mtime, name, and clears dirty', async () => {
    const fixture = await renderEditor(stubClient(), { source: SOURCE });
    const editor = fixture.componentInstance;
    editor.onDraftChange({ ...editor.draft()!, description: 'Changed.' });
    expect(editor.dirty()).toBe(true);

    await editor.save(SOURCE);

    expect(editor.mtime()).toBe(2000);
    expect(editor.location()).toEqual(SOURCE);
    expect(editor.loadedName()).toBe('unlock');
    expect(editor.dirty()).toBe(false);
    expect(toasts().at(-1)).toEqual({
      type: 'success',
      message: 'Pascal has filed the contrivance.',
    });
  });

  it('a CONFLICT opens the reconcile dialog rather than reporting an error', async () => {
    const fixture = await renderEditor(
      stubClient({ writeThrows: () => conflictError() }),
      { source: SOURCE },
    );
    const editor = fixture.componentInstance;

    await editor.save(SOURCE);
    fixture.detectChanges();

    expect(editor.conflictContent()).not.toBeNull();
    expect(toasts().filter((t) => t.type === 'error')).toEqual([]);
    expect(text(fixture)).toContain('The file has moved under your hand');
    expect(text(fixture)).toContain('Reload theirs');
    expect(text(fixture)).toContain('Overwrite with mine');
  });

  it('the force path drops the mtime guard and sends force', async () => {
    const seen: Req[] = [];
    const fixture = await renderEditor(stubClient({ onDispatch: (r) => seen.push(r) }), {
      source: SOURCE,
    });
    await fixture.componentInstance.save(SOURCE, true);

    const write = seen.find((r) => r.type === 'mountFileWrite');
    expect(write?.['force']).toBe(true);
    expect(write?.['expectedMtime']).toBeUndefined();
  });

  it('reload-theirs re-reads the file and re-seeds the editor', async () => {
    const fixture = await renderEditor(
      stubClient({ writeThrows: () => conflictError() }),
      { source: SOURCE },
    );
    const editor = fixture.componentInstance;
    editor.onDraftChange({ ...editor.draft()!, description: 'Mine.' });
    await editor.save(SOURCE);
    expect(editor.conflictContent()).not.toBeNull();

    await editor.reloadTheirs();
    expect(editor.conflictContent()).toBeNull();
    expect(editor.dirty()).toBe(false);
    expect(editor.draft()?.description).toBe('Pick the lock.');
  });

  it('a non-conflict write failure reports rather than opening the dialog', async () => {
    const fixture = await renderEditor(
      stubClient({ writeThrows: () => new Error('disk full') }),
      { source: SOURCE },
    );
    await fixture.componentInstance.save(SOURCE);

    expect(fixture.componentInstance.conflictContent()).toBeNull();
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'disk full' });
  });

  it('a save with no location opens the picker instead of writing', async () => {
    const seen: Req[] = [];
    const fixture = await renderEditor(stubClient({ onDispatch: (r) => seen.push(r) }), {
      create: { template: VALID_DEFINITION },
    });
    await fixture.componentInstance.handleSave();

    expect(fixture.componentInstance.pickerOpen()).toBe('save');
    expect(seen.some((r) => r.type === 'mountFileWrite')).toBe(false);
  });

  it('saveToStore refuses to clobber a file already at the target path', async () => {
    const seen: Req[] = [];
    const fixture = await renderEditor(
      stubClient({ onDispatch: (r) => seen.push(r), content: VALID_DEFINITION }),
      { create: { template: VALID_DEFINITION } },
    );
    fixture.componentInstance.pickerOpen.set('save');
    // The existence probe succeeds → the path is taken.
    await fixture.componentInstance.saveToStore({ mountPointId: 'm9', mountName: 'General' });

    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'General already holds Tools/unlock.tool.json — rename this contrivance first.',
    });
    expect(seen.some((r) => r.type === 'mountFileWrite')).toBe(false);
  });

  it('saveToStore writes Tools/<name>.tool.json when the path is free', async () => {
    const seen: Req[] = [];
    const fixture = await renderEditor(
      stubClient({ onDispatch: (r) => seen.push(r), readThrows: () => notFoundError() }),
      { create: { template: VALID_DEFINITION } },
    );
    fixture.componentInstance.pickerOpen.set('save');
    await fixture.componentInstance.saveToStore({ mountPointId: 'm9', mountName: 'General' });

    const write = seen.find((r) => r.type === 'mountFileWrite');
    expect(write).toMatchObject({ mountPointId: 'm9', path: 'Tools/unlock.tool.json' });
  });

  it('switching to form is refused while the JSON does not validate', async () => {
    const fixture = await renderEditor(stubClient(), { source: SOURCE });
    const editor = fixture.componentInstance;
    editor.switchToJson();
    editor.jsonText.set('{ not json');
    await settle(fixture);

    editor.switchToForm();
    expect(editor.editorMode()).toBe('json');
  });

  it('JSON → form clears the repair reason', async () => {
    const fixture = await renderEditor(stubClient({ content: '{ not json' }), { source: SOURCE });
    const editor = fixture.componentInstance;
    expect(editor.repairReason()).not.toBeNull();

    editor.jsonText.set(VALID_DEFINITION);
    editor.switchToForm();

    expect(editor.editorMode()).toBe('form');
    expect(editor.repairReason()).toBeNull();
  });

  it('leaving with unsaved changes asks first', async () => {
    const fixture = await renderEditor(stubClient(), { source: SOURCE });
    const editor = fixture.componentInstance;
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    let backs = 0;
    editor.back.subscribe(() => (backs += 1));

    editor.handleBack();
    expect(backs).toBe(1); // clean — no prompt
    expect(confirm).not.toHaveBeenCalled();

    editor.onDraftChange({ ...editor.draft()!, description: 'Changed.' });
    editor.handleBack();
    expect(confirm).toHaveBeenCalledOnce();
    expect(backs).toBe(1); // declined

    confirm.mockReturnValue(true);
    editor.handleBack();
    expect(backs).toBe(2);
    confirm.mockRestore();
  });
});

describe('DestinationPicker (v4 DestinationPicker.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  async function renderPicker(
    toolName: string,
    destinations: unknown,
  ): Promise<ComponentFixture<DestinationPicker>> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [DestinationPicker],
      providers: [{ provide: CoreClient, useValue: stubClient({ destinations }) }],
    });
    const fixture = TestBed.createComponent(DestinationPicker);
    fixture.componentRef.setInput('toolName', toolName);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  const TREE = {
    general: { mountPointId: 'm1', mountName: 'General', existingToolNames: ['taken'] },
    projects: [
      {
        projectId: 'p1',
        projectName: 'Ledger',
        stores: [{ mountPointId: 'm2', mountName: 'Ledger store', existingToolNames: [] }],
      },
    ],
    groups: [
      {
        groupId: 'g1',
        groupName: 'The Salon',
        stores: [
          {
            mountPointId: 'm3',
            mountName: 'Salon store',
            existingToolNames: [],
            official: true,
          },
        ],
      },
    ],
    characters: [
      {
        characterId: 'c1',
        characterName: 'Aria',
        mountPointId: 'm4',
        mountName: "Aria's vault",
        existingToolNames: [],
      },
    ],
    other: [{ mountPointId: 'm5', mountName: 'Orphan', existingToolNames: [] }],
  };

  it('renders every group with its consequence line, and stars the official store', async () => {
    const fixture = await renderPicker('unlock', TREE);
    const body = text(fixture);

    expect(body).toContain('◈ The General Store');
    expect(body).toContain('every chat, every character');
    expect(body).toContain('◈ Projects');
    expect(body).toContain('chats in that project');
    expect(body).toContain('◈ Groups');
    expect(body).toContain('Official Store ★');
    expect(body).toContain('◈ Character vaults');
    expect(body).toContain('shadows every farther tier');
    expect(body).toContain('◈ Other stores');
    expect(body).toContain('inert until linked');
  });

  it('says so when the General store is unprovisioned', async () => {
    const fixture = await renderPicker('unlock', { ...TREE, general: null });
    expect(text(fixture)).toContain('Not yet provisioned');
  });

  it('a duplicate in the SELECTED store BLOCKS the save', async () => {
    const fixture = await renderPicker('taken', TREE);
    const picker = fixture.componentInstance;

    picker.selected.set({ mountPointId: 'm1', mountName: 'General' });
    fixture.detectChanges();

    expect(picker.duplicateHere()).toBe(true);
    const keep = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Keep it here',
    );
    expect(keep?.disabled).toBe(true);
    expect(text(fixture)).toContain('both be refused at deal time');
    expect(text(fixture)).toContain('Open the existing one instead');
  });

  it('the same name in ANOTHER store is a non-blocking advisory', async () => {
    const fixture = await renderPicker('taken', TREE);
    const picker = fixture.componentInstance;

    picker.selected.set({ mountPointId: 'm2', mountName: 'Ledger store' });
    fixture.detectChanges();

    expect(picker.duplicateHere()).toBe(false);
    expect(picker.nameElsewhere()).toBe(true);
    const keep = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Keep it here',
    );
    expect(keep?.disabled).toBe(false);
    expect(text(fixture)).toContain('the nearest tier wins');
  });

  it('nothing selected also blocks the save', async () => {
    const fixture = await renderPicker('unlock', TREE);
    const keep = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Keep it here',
    );
    expect(keep?.disabled).toBe(true);
  });
});

describe('CustomToolsPage (v4 page.tsx + CustomToolsView.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  /** A minimal ActivatedRoute whose snapshot carries the deep-link query. */
  function routeStub(query: string): { snapshot: { queryParamMap: ParamMap } } {
    const params = new URLSearchParams(query);
    return {
      snapshot: {
        queryParamMap: {
          get: (key: string) => params.get(key),
          has: (key: string) => params.has(key),
          getAll: (key: string) => params.getAll(key),
          get keys() {
            return [...params.keys()];
          },
        } as ParamMap,
      },
    };
  }

  async function renderPage(query: string): Promise<ComponentFixture<CustomToolsPage>> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [CustomToolsPage],
      providers: [
        provideRouter([]),
        { provide: CoreClient, useValue: stubClient() },
        { provide: ActivatedRoute, useValue: routeStub(query) },
      ],
    });
    const fixture = TestBed.createComponent(CustomToolsPage);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('lands on the library with no query params', async () => {
    const fixture = await renderPage('');
    expect(fixture.componentInstance.mode()).toEqual({ view: 'library' });
  });

  it('drilling a definition renders the editor IN PLACE, not a route change', async () => {
    const fixture = await renderPage('');
    fixture.componentInstance.mode.set({
      view: 'edit',
      mountPointId: 'm1',
      path: 'Tools/unlock.tool.json',
    });
    fixture.detectChanges();
    await settle(fixture);

    expect(fixture.componentInstance.editSource()).toEqual({
      mountPointId: 'm1',
      path: 'Tools/unlock.tool.json',
    });
  });

  it('?mount=&path= deep-links straight into edit mode (v4 own fallback)', async () => {
    const fixture = await renderPage('mount=m1&path=Tools/unlock.tool.json');
    expect(fixture.componentInstance.mode()).toEqual({
      view: 'edit',
      mountPointId: 'm1',
      path: 'Tools/unlock.tool.json',
    });
  });

  it('?new=1&mount= deep-links into create with the destination preselected', async () => {
    const fixture = await renderPage('new=1&mount=m7');
    expect(fixture.componentInstance.mode()).toEqual({ view: 'create', mountPointId: 'm7' });
  });

  it('?new=1 alone creates with no destination preselected', async () => {
    const fixture = await renderPage('new=1');
    expect(fixture.componentInstance.mode()).toEqual({ view: 'create', mountPointId: undefined });
  });

  it('a half-specified deep link falls back to the library', async () => {
    const onlyMount = await renderPage('mount=m1');
    expect(onlyMount.componentInstance.mode()).toEqual({ view: 'library' });

    const onlyPath = await renderPage('path=Tools/unlock.tool.json');
    expect(onlyPath.componentInstance.mode()).toEqual({ view: 'library' });
  });

  it('the duplicate flow carries the template into create mode', async () => {
    const fixture = await renderPage('');
    fixture.componentInstance.mode.set({ view: 'create', template: VALID_DEFINITION });
    fixture.detectChanges();

    expect(fixture.componentInstance.createSpec()).toEqual({
      mountPointId: undefined,
      template: VALID_DEFINITION,
    });
  });
});

/**
 * The editor's wardrobe vantage (v4 `5cb0e0190`, `WorkbenchEditor.tsx` +50;
 * P4.D320 R-10): seeded from the store the file lives in, re-seeded when a
 * Save As… moves it, latched the moment the author chooses by hand, and never
 * written to the file. v4 ships no editor test for it.
 */
describe('WorkbenchEditor — the wardrobe vantage (v4 5cb0e0190)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const store = (mountPointId: string) => ({ mountPointId, mountName: mountPointId, existingToolNames: [] });
  const DESTINATIONS = {
    general: store('mp-general'),
    characters: [{ characterId: 'char-imogen', characterName: 'Imogen', ...store('mp-imogen') }],
    groups: [{ groupId: 'g-1', groupName: 'The Regulars', stores: [{ ...store('mp-group'), official: true }] }],
    projects: [
      { projectId: 'p-1', projectName: 'Thornfield', stores: [store('mp-proj'), store('mp-imogen')] },
    ],
    other: [],
  };
  const IMOGEN_SOURCE = { mountPointId: 'mp-imogen', path: 'Tools/unlock.tool.json' };

  /** Reads answer only the opened file; any other path is free (a Save As target). */
  function lensClient(seen: Req[] = []): Partial<CoreClient> {
    return {
      dispatchData: (async (req: Req) => {
        seen.push(req);
        if (req.type === 'mountFileRead') {
          if (req['mountPointId'] === IMOGEN_SOURCE.mountPointId && req['path'] === IMOGEN_SOURCE.path) {
            return { content: VALID_DEFINITION, mtime: 1000 };
          }
          throw notFoundError();
        }
        if (req.type === 'mountFileWrite') return { mtime: 2000 };
        if (req.type === 'customToolsDestinations') return DESTINATIONS;
        return {};
      }) as CoreClient['dispatchData'],
    };
  }

  it("opens a character vault's file on that character", async () => {
    const fixture = await renderEditor(lensClient(), { source: IMOGEN_SOURCE });
    expect(fixture.componentInstance.vantage()).toEqual({ kind: 'character', characterId: 'char-imogen' });
  });

  it('opens an unsaved file on nobody', async () => {
    const fixture = await renderEditor(lensClient(), { create: {} });
    expect(fixture.componentInstance.vantage()).toEqual({ kind: 'none' });
  });

  it('follows the file into a group store after Save As…', async () => {
    const fixture = await renderEditor(lensClient(), { source: IMOGEN_SOURCE });
    const editor = fixture.componentInstance;
    editor.pickerOpen.set('save-as');
    await editor.saveToStore({ mountPointId: 'mp-group', mountName: 'The Regulars' });
    await settle(fixture);
    expect(editor.location()?.mountPointId).toBe('mp-group');
    expect(editor.vantage()).toEqual({ kind: 'container', container: { scope: 'group', id: 'g-1' } });
  });

  it('stops following once the author chooses by hand', async () => {
    const fixture = await renderEditor(lensClient(), { source: IMOGEN_SOURCE });
    const editor = fixture.componentInstance;
    editor.chooseVantage({ kind: 'container', container: { scope: 'general', id: null } });
    editor.pickerOpen.set('save-as');
    await editor.saveToStore({ mountPointId: 'mp-group', mountName: 'The Regulars' });
    await settle(fixture);
    expect(editor.location()?.mountPointId).toBe('mp-group');
    expect(editor.vantage()).toEqual({ kind: 'container', container: { scope: 'general', id: null } });
  });

  it('names the home project only under a character vantage', async () => {
    const fixture = await renderEditor(lensClient(), { source: IMOGEN_SOURCE });
    const editor = fixture.componentInstance;
    expect(editor.homeProjectName()).toBe('Thornfield');
    // Give the draft a wardrobe row so the section shows its vantage select.
    const current = editor.draft()!;
    editor.onDraftChange({
      ...current,
      effects: [
        {
          id: 'w1',
          when: { kind: 'always' },
          kind: 'wardrobe',
          target: '',
          valueKind: 'expression',
          value: '',
          wardrobe: { action: 'wear', items: [''], mode: 'wear', slot: '' },
          inventory: emptyDraftInventoryGesture(),
          log: '',
        },
      ],
    });
    await settle(fixture);
    const hint = () =>
      (fixture.nativeElement as HTMLElement).querySelector('qt-side-effects-section qt-wardrobe-vantage-select p')!
        .textContent!;
    expect(hint()).toContain('This tool lives in Thornfield’s store');

    editor.chooseVantage({ kind: 'container', container: { scope: 'general', id: null } });
    await settle(fixture);
    expect(hint()).not.toContain('This tool lives in');
  });

  it('never writes the vantage into the file', async () => {
    const writes: string[] = [];
    for (const vantage of [
      { kind: 'character', characterId: 'char-imogen' },
      { kind: 'container', container: { scope: 'group', id: 'g-1' } },
      { kind: 'none' },
    ] as const) {
      const seen: Req[] = [];
      const fixture = await renderEditor(lensClient(seen), { source: IMOGEN_SOURCE });
      const editor = fixture.componentInstance;
      editor.chooseVantage(vantage);
      editor.pickerOpen.set('save-as');
      await editor.saveToStore({ mountPointId: 'mp-proj', mountName: 'Thornfield' });
      writes.push(seen.find((r) => r.type === 'mountFileWrite')!['content'] as string);
      TestBed.resetTestingModule();
    }
    expect(new Set(writes).size).toBe(1);
    expect(writes[0]).not.toContain('char-imogen');
  });
});
