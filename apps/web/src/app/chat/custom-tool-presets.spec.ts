import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError } from '../core/core-contract';
import { CustomToolsPopup } from './custom-tools-popup';
import type { CustomToolRosterEntry, CustomToolsRoster } from './custom-tools.api';
import { ToastService } from '../ui/toast.service';

/**
 * Run presets in the composer's run dialog (v4 `ToolPresetsSection`,
 * `c988fbd2`) — driven through the POPUP rather than the section alone, because
 * three of the ten behaviors are the wiring: the render gate, the value
 * replacement landing on the popup's own `formValues`, and the section
 * unmounting (so a typed name resets) when the operator goes back to the picker.
 *
 * Nothing here mocks the naming contract: the paths asserted are the bytes the
 * dialog actually asks the mount verbs for, and `tool-presets.spec.ts` proves
 * the contract itself against v4's own suite.
 */

const VAULT = 'vault-charlie';

/** A refusal in the shape the transport actually raises. */
function dispatchError(message: string): CoreDispatchError {
  return new CoreDispatchError({ kind: 'internal', message });
}

/** A tool with one number and one boolean parameter, run as a character with a vault. */
const UNLOCK: CustomToolRosterEntry = {
  name: 'unlock',
  title: 'Force the Lock',
  description: 'Attempt to pick the lock.',
  parameters: {
    bonus: { type: 'number', default: 0, description: 'Skill bonus added to the roll.' },
    quiet: { type: 'boolean', default: false, description: 'Work without noise.' },
  },
  defaultVisibility: 'public',
  sourceTier: 'project',
  asCharacterId: 'char-1',
  vaultMountPointId: VAULT,
  definitionPath: 'Tools/unlock.tool.json',
  mountPointId: 'mount-1',
  mountName: 'Kepler Notes',
};

/** No parameters — nothing a preset could preserve. */
const SPIN: CustomToolRosterEntry = {
  ...UNLOCK,
  name: 'spin',
  title: 'Spin',
  parameters: {},
  definitionPath: 'Tools/spin.tool.json',
};

/** Parameters, but the running character's vault could not be resolved. */
const VAULTLESS: CustomToolRosterEntry = { ...UNLOCK, vaultMountPointId: null };

interface Stub {
  calls: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

interface StubOptions {
  /** Vault-relative paths `mountFilesList` answers with. */
  files?: string[];
  /** The bytes `mountFileRead` answers with, keyed by path. */
  reads?: Record<string, string>;
  /** Thrown instead of answering, keyed by verb. */
  fail?: Partial<Record<'mountFilesList' | 'mountFileRead' | 'mountFileWrite', Error>>;
}

function stub(tools: CustomToolRosterEntry[], options: StubOptions = {}): Stub {
  const calls: Record<string, unknown>[] = [];
  const files = [...(options.files ?? [])];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    const type = req['type'] as string;
    const failure = options.fail?.[type as keyof NonNullable<StubOptions['fail']>];
    if (failure) throw failure;
    if (type === 'chatCustomToolsList') {
      return { tools, errors: [] } as unknown as CustomToolsRoster as unknown as Record<
        string,
        unknown
      >;
    }
    if (type === 'mountFilesList') {
      return { files: files.map((relativePath, i) => ({ id: `f${i}`, relativePath })) };
    }
    if (type === 'mountFileRead') {
      return { content: options.reads?.[req['path'] as string] ?? '{}' };
    }
    if (type === 'mountFileWrite') {
      // A save shows up in the next listing, as a real vault would.
      const path = req['path'] as string;
      if (!files.includes(path)) files.push(path);
      return {};
    }
    return {};
  });
  return {
    calls,
    client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] },
  };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(s: Stub): Promise<ComponentFixture<CustomToolsPopup>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CustomToolsPopup],
    providers: [
      // `retry: false`: the refusal arms below assert what the section renders
      // ONCE a read has failed, and TanStack's default three retries would
      // otherwise still be backing off when the fixture settles.
      provideTanStackQuery(
        new QueryClient({ defaultOptions: { queries: { retry: false } } }),
      ),
      { provide: CoreClient, useValue: s.client },
    ],
  });
  const fixture = TestBed.createComponent(CustomToolsPopup);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function q<T extends Element>(fixture: ComponentFixture<unknown>, selector: string): T | null {
  return fixture.nativeElement.querySelector(selector) as T | null;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
}

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

/**
 * Open the dialog and choose the named tool — the dialog's form phase.
 *
 * The title is REQUIRED and the row lookup throws when it misses: an earlier
 * draft fell back to "whatever button is second", which silently clicked the
 * modal's ✕ for any tool not called "Force the Lock" and left three assertions
 * passing against a dialog that had never reached its form phase at all.
 */
async function openForm(
  s: Stub,
  title = 'Force the Lock',
): Promise<ComponentFixture<CustomToolsPopup>> {
  const fixture = await mount(s);
  q<HTMLButtonElement>(fixture, 'button[aria-label="Custom tools"]')!.click();
  fixture.detectChanges();
  await settle(fixture);
  const row = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLButtonElement) =>
    (b.textContent ?? '').includes(title),
  ) as HTMLButtonElement | undefined;
  if (!row) throw new Error(`openForm: no roster row for ${title}`);
  row.click();
  fixture.detectChanges();
  await settle(fixture);
  // The form phase, not the picker — the Run button only exists on the far side.
  expect(findButton(fixture, `Run ${title}`)).toBeTruthy();
  return fixture;
}

function presetSelect(fixture: ComponentFixture<unknown>): HTMLSelectElement | null {
  return q<HTMLSelectElement>(fixture, 'select[aria-label^="Load a saved preset"]');
}

function nameInput(fixture: ComponentFixture<unknown>): HTMLInputElement | null {
  return q<HTMLInputElement>(fixture, 'input[aria-label="Name to save this preset under"]');
}

function findButton(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  return [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLButtonElement) =>
    (b.textContent ?? '').trim().startsWith(label),
  ) as HTMLButtonElement;
}

/** The bonus field's current value, as the parameter form renders it. */
function bonusField(fixture: ComponentFixture<unknown>): HTMLInputElement {
  return q<HTMLInputElement>(fixture, 'input[type="number"]')!;
}

async function type(
  fixture: ComponentFixture<unknown>,
  el: HTMLInputElement,
  value: string,
): Promise<void> {
  el.value = value;
  el.dispatchEvent(new Event('input'));
  fixture.detectChanges();
  await settle(fixture);
}

async function choose(
  fixture: ComponentFixture<unknown>,
  el: HTMLSelectElement,
  value: string,
): Promise<void> {
  el.value = value;
  el.dispatchEvent(new Event('change'));
  fixture.detectChanges();
  await settle(fixture);
}

async function click(fixture: ComponentFixture<unknown>, el: HTMLElement): Promise<void> {
  el.click();
  fixture.detectChanges();
  await settle(fixture);
}

describe('run presets — when the section is dealt at all', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders for a tool with parameters run as a character with a vault', async () => {
    const fixture = await openForm(stub([UNLOCK]));
    expect(presetSelect(fixture)).not.toBeNull();
    expect(text(fixture)).toContain('Presets');
  });

  it('stays hidden for a tool that declares no parameters', async () => {
    const fixture = await openForm(stub([SPIN]), 'Spin');
    expect(presetSelect(fixture)).toBeNull();
  });

  it('stays hidden when the running character has no vault (vaultMountPointId null)', async () => {
    const fixture = await openForm(stub([VAULTLESS]));
    expect(presetSelect(fixture)).toBeNull();
  });

  it('stays hidden on a roster from an older server, where the field is absent', async () => {
    const { vaultMountPointId: _omitted, ...legacy } = UNLOCK;
    const fixture = await openForm(stub([legacy as CustomToolRosterEntry]));
    expect(presetSelect(fixture)).toBeNull();
  });

  it('sits between the parameters and the privacy toggle (v4 position)', async () => {
    const fixture = await openForm(stub([UNLOCK]));
    const html = fixture.nativeElement.innerHTML as string;
    expect(html.indexOf('What you are setting')).toBeLessThan(html.indexOf('Presets'));
    expect(html.indexOf('Presets')).toBeLessThan(html.indexOf('Roll privately'));
  });

  it('renders as a block, so the popup’s vertical rhythm applies to it', async () => {
    // An unknown element defaults to `display: inline`, which drops the
    // parent's `space-y-5` margin entirely (the `qt-collapsible-card` /
    // P4.9E1B bug class). The component pins `:host { display: block }`.
    const fixture = await openForm(stub([UNLOCK]));
    const host = fixture.nativeElement.querySelector('qt-custom-tool-presets') as HTMLElement;
    expect(host).not.toBeNull();
    expect(getComputedStyle(host).display).toBe('block');
  });
});

describe('run presets — listing', () => {
  afterEach(() => TestBed.resetTestingModule());

  it("lists only this tool's presets, sorted, and never a definition file", async () => {
    const fixture = await openForm(
      stub([UNLOCK], {
        files: [
          'Tools/unlock.tool.json',
          'Tools/unlock.quiet-run.settings.json',
          'Tools/unlock.brute.settings.json',
          // Another tool's preset, a nested one, and an illegal segment.
          'Tools/spin.brute.settings.json',
          'Tools/deep/unlock.nested.settings.json',
          'Tools/unlock.Loud.settings.json',
        ],
      }),
    );
    const options = [...presetSelect(fixture)!.querySelectorAll('option')].map((o) => o.value);
    expect(options).toEqual(['', 'brute', 'quiet-run']);
  });

  it('asks the vault mount, not the definition mount', async () => {
    const s = stub([UNLOCK], { files: [] });
    await openForm(s);
    const list = s.calls.find((c) => c['type'] === 'mountFilesList');
    expect(list?.['mountPointId']).toBe(VAULT);
  });

  it('says so, inline, when the vault would not list', async () => {
    const fixture = await openForm(
      stub([UNLOCK], { fail: { mountFilesList: dispatchError('Vault offline.') } }),
    );
    expect(text(fixture)).toContain('Vault offline.');
    // …and the dropdown has nothing to offer.
    expect(presetSelect(fixture)!.disabled).toBe(true);
  });
});

describe('run presets — the placeholder and the enablement matrix', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers "No presets yet" on an empty vault, with the select disabled', async () => {
    const fixture = await openForm(stub([UNLOCK], { files: [] }));
    expect(presetSelect(fixture)!.querySelector('option')!.textContent).toContain('No presets yet');
    expect(presetSelect(fixture)!.disabled).toBe(true);
  });

  it('offers "Load a preset…" once there is one, and enables the select', async () => {
    const fixture = await openForm(
      stub([UNLOCK], { files: ['Tools/unlock.brute.settings.json'] }),
    );
    expect(presetSelect(fixture)!.querySelector('option')!.textContent).toContain(
      'Load a preset…',
    );
    expect(presetSelect(fixture)!.disabled).toBe(false);
  });

  it('refuses to save until the typed name passes the pattern', async () => {
    const fixture = await openForm(stub([UNLOCK], { files: [] }));
    const save = findButton(fixture, 'Save preset');
    expect(save.disabled).toBe(true);

    // The keystroke filter strips the illegal characters as typed…
    await type(fixture, nameInput(fixture)!, 'Hard Mode!');
    expect(nameInput(fixture)!.value).toBe('hardmode');
    expect(findButton(fixture, 'Save preset').disabled).toBe(false);

    // …but cannot fix a leading dash, which the pattern still refuses.
    await type(fixture, nameInput(fixture)!, '-x');
    expect(nameInput(fixture)!.value).toBe('-x');
    expect(findButton(fixture, 'Save preset').disabled).toBe(true);
  });
});

describe('run presets — saving', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('writes the COERCED values, pretty-printed, at the contract path', async () => {
    const s = stub([UNLOCK], { files: [] });
    const fixture = await openForm(s);
    await type(fixture, bonusField(fixture), '3');
    await type(fixture, nameInput(fixture)!, 'brute');
    await click(fixture, findButton(fixture, 'Save preset'));

    const write = s.calls.find((c) => c['type'] === 'mountFileWrite');
    expect(write?.['mountPointId']).toBe(VAULT);
    expect(write?.['path']).toBe('Tools/unlock.brute.settings.json');
    // Coerced: the number field is a number, the boolean a boolean.
    expect(write?.['content']).toBe(JSON.stringify({ bonus: 3, quiet: false }, null, 2));
    expect(toasts().at(-1)).toEqual({
      type: 'success',
      message: 'Preset “brute” filed in the character\'s vault.',
    });
  });

  it('refreshes the list so the new preset is immediately loadable', async () => {
    const s = stub([UNLOCK], { files: [] });
    const fixture = await openForm(s);
    await type(fixture, nameInput(fixture)!, 'brute');
    await click(fixture, findButton(fixture, 'Save preset'));
    const options = [...presetSelect(fixture)!.querySelectorAll('option')].map((o) => o.value);
    expect(options).toEqual(['', 'brute']);
  });

  it('names the failure when the write is refused', async () => {
    const fixture = await openForm(
      stub([UNLOCK], { files: [], fail: { mountFileWrite: dispatchError('Vault is read-only.') } }),
    );
    await type(fixture, nameInput(fixture)!, 'brute');
    await click(fixture, findButton(fixture, 'Save preset'));
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Vault is read-only.' });
  });
});

describe('run presets — loading, loosely bound', () => {
  afterEach(() => TestBed.resetTestingModule());

  const loaded = (over: Record<string, unknown>) =>
    stub([UNLOCK], {
      files: ['Tools/unlock.brute.settings.json'],
      reads: { 'Tools/unlock.brute.settings.json': JSON.stringify(over) },
    });

  it('deals the saved values back into the form', async () => {
    const fixture = await openForm(loaded({ bonus: 4, quiet: true }));
    await choose(fixture, presetSelect(fixture)!, 'brute');
    expect(bonusField(fixture).value).toBe('4');
    expect(q<HTMLInputElement>(fixture, 'input[type="checkbox"]')!.checked).toBe(true);
  });

  it('copies the loaded name into the input, so re-saving updates it', async () => {
    const fixture = await openForm(loaded({ bonus: 4 }));
    await choose(fixture, presetSelect(fixture)!, 'brute');
    expect(nameInput(fixture)!.value).toBe('brute');
  });

  it('leaves a parameter the preset does not mention at its current value', async () => {
    const fixture = await openForm(loaded({ quiet: true }));
    await type(fixture, bonusField(fixture), '2');
    await choose(fixture, presetSelect(fixture)!, 'brute');
    expect(bonusField(fixture).value).toBe('2');
  });

  it('ignores a key the tool no longer declares, and a non-primitive value', async () => {
    const fixture = await openForm(loaded({ bonus: { nested: 1 }, retired: 'gone', quiet: true }));
    await choose(fixture, presetSelect(fixture)!, 'brute');
    // bonus stayed at its default: the object was skipped, not stringified.
    expect(bonusField(fixture).value).toBe('0');
    expect(q<HTMLInputElement>(fixture, 'input[type="checkbox"]')!.checked).toBe(true);
  });

  it('refuses a file that would not read as JSON, leaving it as written', async () => {
    const fixture = await openForm(
      stub([UNLOCK], {
        files: ['Tools/unlock.brute.settings.json'],
        reads: { 'Tools/unlock.brute.settings.json': '{ not json' },
      }),
    );
    await choose(fixture, presetSelect(fixture)!, 'brute');
    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'Preset “brute” would not read as JSON. It stays as written in the vault.',
    });
  });

  it('refuses a file that parses but is not a settings object', async () => {
    const fixture = await openForm(loaded([] as unknown as Record<string, unknown>));
    await choose(fixture, presetSelect(fixture)!, 'brute');
    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'Preset “brute” is not a settings object. It stays as written in the vault.',
    });
  });

  it('names the failure when the read is refused', async () => {
    const fixture = await openForm(
      stub([UNLOCK], {
        files: ['Tools/unlock.brute.settings.json'],
        fail: { mountFileRead: dispatchError('No such file.') },
      }),
    );
    await choose(fixture, presetSelect(fixture)!, 'brute');
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'No such file.' });
  });
});

describe('run presets — resetting, and what a close forgets', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('re-seeds the form from the declared defaults', async () => {
    const fixture = await openForm(stub([UNLOCK], { files: [] }));
    await type(fixture, bonusField(fixture), '5');
    await click(fixture, findButton(fixture, 'Reset to defaults'));
    expect(bonusField(fixture).value).toBe('0');
  });

  it('forgets the typed name when the operator goes back to the picker', async () => {
    const fixture = await openForm(stub([UNLOCK], { files: [] }));
    await type(fixture, nameInput(fixture)!, 'brute');
    await click(fixture, findButton(fixture, 'Choose another tool'));
    // …and back into the same tool's form.
    await click(
      fixture,
      [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLButtonElement) =>
        (b.textContent ?? '').includes('Force the Lock'),
      ) as HTMLButtonElement,
    );
    expect(nameInput(fixture)!.value).toBe('');
    // The VALUES, by contrast, are the popup's memory and survive (v4 too).
    expect(bonusField(fixture)).not.toBeNull();
  });
});
