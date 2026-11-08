import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import {
  CoreDispatchError,
  type ChatCustomToolPresetsRequest,
  type CustomToolParameterSpec,
  type CustomToolPresetPool,
} from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { CustomToolPresets } from './custom-tool-presets';
import type { ParameterFormValues } from './custom-tool-params-form';
import { customToolsKeys } from './custom-tools.api';

/**
 * Run presets in the composer's run dialog (v4 `ToolPresetsSection`,
 * `fd165fce0`, at the pin `624d0d25c`): the list rides the server's tool walk
 * (`chatCustomToolPresets` — contract C11), loads and saves aim at the
 * preset's OWN store.
 *
 * Driven through a host that binds the section exactly as P4.D388's HANDOFF H1
 * patch makes `custom-tools-popup.ts` bind it (`[chatId]`, `[asCharacterId]`,
 * `[toolKey]`, `[vaultMountPointId]`). The popup's own wiring — its render
 * gate, the section's position, the name reset on going back — is the popup's
 * file (P4.D385), proven on the union by `e2e/spa-smalls-flow.spec.ts` (H) and
 * the gated presets beat of `salon-custom-tools-flow.spec.ts`.
 *
 * Nothing here mocks the naming contract: the save path asserted is the bytes
 * the section asks `mountFileWrite` for, and `tool-presets.spec.ts` proves the
 * contract itself against v4's own suite.
 */

// A type-level pin of the C11 request (P4.D388 item 2).
const _request: ChatCustomToolPresetsRequest = { type: 'chatCustomToolPresets', chatId: 'c' };
void _request;

const PARAMS: Record<string, CustomToolParameterSpec> = {
  bonus: { type: 'number', default: 0, description: 'Skill bonus added to the roll.' },
  quiet: { type: 'boolean', default: false, description: 'Work without noise.' },
};

const VAULT = { tier: 'character', mountPointId: 'vault-charlie', mountName: 'Charlie Vault' };
const GROUP = { tier: 'group', mountPointId: 'group-1', mountName: 'The Ring' };
const GENERAL = { tier: 'general', mountPointId: 'general', mountName: 'Quilltap General' };

const POOL: CustomToolPresetPool = {
  presets: [
    { ...GROUP, name: 'brute', path: 'Tools/unlock.brute.settings.json', shadows: [GENERAL] },
    { ...VAULT, name: 'sneak', path: 'Tools/unlock.sneak.settings.json', shadows: [] },
  ],
  stores: [VAULT, GROUP, GENERAL],
  unread: [],
};

interface StubOptions {
  pool?: Partial<CustomToolPresetPool> | Record<string, unknown>;
  reads?: Record<string, string>;
  fail?: Partial<Record<'chatCustomToolPresets' | 'mountFileRead' | 'mountFileWrite', Error>>;
}

function stub(options: StubOptions = {}) {
  const calls: Record<string, unknown>[] = [];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    const type = req['type'] as string;
    const failure = options.fail?.[type as keyof NonNullable<StubOptions['fail']>];
    if (failure) throw failure;
    if (type === 'chatCustomToolPresets') return (options.pool ?? POOL) as Record<string, unknown>;
    if (type === 'mountFileRead') {
      return { content: options.reads?.[`${req['mountPointId']}:${req['path']}`] ?? '{}' };
    }
    return {};
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

@Component({
  imports: [CustomToolPresets],
  template: `
    <qt-custom-tool-presets
      [chatId]="chatId()"
      [asCharacterId]="asCharacterId()"
      [toolKey]="'unlock::char-1'"
      [vaultMountPointId]="vault()"
      toolName="unlock"
      toolTitle="Force the Lock"
      [parameters]="params"
      [values]="values()"
      [disabled]="disabled()"
      (valuesReplace)="values.set($event)"
    />
  `,
})
class Host {
  readonly chatId = signal<string | null>('chat-1');
  readonly asCharacterId = signal<string | null>('char-1');
  readonly vault = signal<string | null>('vault-charlie');
  readonly disabled = signal(false);
  readonly params = PARAMS;
  readonly values = signal<ParameterFormValues>({ bonus: '0', quiet: false });
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(
  s: ReturnType<typeof stub>,
  setup: (h: Host) => void = () => undefined,
): Promise<{ fixture: ComponentFixture<Host>; client: QueryClient }> {
  TestBed.resetTestingModule();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [provideTanStackQuery(client), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, client };
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const text = (f: ComponentFixture<unknown>) => (el(f).textContent ?? '').replace(/\s+/g, ' ').trim();
const loadSelect = (f: ComponentFixture<unknown>) =>
  el(f).querySelector('select[aria-label^="Load a saved preset"]') as HTMLSelectElement | null;
const storeSelect = (f: ComponentFixture<unknown>) =>
  el(f).querySelector('#custom-tool-unlock\\:\\:char-1-preset-store') as HTMLSelectElement | null;
const nameInput = (f: ComponentFixture<unknown>) =>
  el(f).querySelector('input[aria-label="Name to save this preset under"]') as HTMLInputElement;
const button = (f: ComponentFixture<unknown>, label: string) =>
  Array.from(el(f).querySelectorAll('button')).find((b) =>
    (b.textContent ?? '').trim().startsWith(label),
  ) as HTMLButtonElement;
const optionText = (o: HTMLOptionElement) => (o.textContent ?? '').replace(/\s+/g, ' ').trim();
const toasts = () =>
  TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));

async function typeName(f: ComponentFixture<unknown>, value: string): Promise<void> {
  const input = nameInput(f);
  input.value = value;
  input.dispatchEvent(new Event('input'));
  await settle(f);
}

async function choose(f: ComponentFixture<unknown>, s: HTMLSelectElement, value: string): Promise<void> {
  s.value = value;
  s.dispatchEvent(new Event('change'));
  await settle(f);
}

describe('run presets — the walk (v4 fd165fce0)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders nothing and dispatches nothing without a chat or a character (PR-6)', async () => {
    for (const setup of [
      (h: Host) => h.chatId.set(null),
      (h: Host) => h.asCharacterId.set(null),
    ]) {
      const s = stub();
      const { fixture } = await mount(s, setup);
      expect(el(fixture).querySelector('section')).toBeNull();
      expect(s.calls).toEqual([]);
    }
  });

  it('asks the presets verb for this tool as this character', async () => {
    const s = stub();
    await mount(s);
    expect(s.calls).toEqual([
      { type: 'chatCustomToolPresets', chatId: 'chat-1', tool: 'unlock', asCharacterId: 'char-1' },
    ]);
  });

  it("keys the query v4's way on v5's prefix, under byChat", () => {
    expect(customToolsKeys.presets('chat-1', 'char-1', 'unlock')).toEqual([
      'customTools',
      'chat-1',
      'presets',
      'char-1',
      'unlock',
    ]);
    expect(customToolsKeys.presets('chat-1', null, 'unlock')[3]).toBe('');
    expect(customToolsKeys.presets('c', 'a', 't').slice(0, 2)).toEqual(customToolsKeys.byChat('c'));
  });

  it("carries v4's paragraph", async () => {
    const { fixture } = await mount(stub());
    expect(el(fixture).querySelector('section p')!.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'A hand you set aside earlier, dealt back on request. They are kept wherever the tool itself might be: the character’s vault, their groups’ stores, the project’s, or Quilltap General — and the nearest one of a name wins. Saving under an existing name in the same store replaces it.',
    );
  });

  it('lists each preset as "name — store", with the shadows title only when shadowed', async () => {
    const { fixture } = await mount(stub());
    const options = Array.from(loadSelect(fixture)!.options).slice(1);
    expect(options.map(optionText)).toEqual(['brute — The Ring', 'sneak — Charlie Vault']);
    expect(options.map((o) => o.value)).toEqual(['brute', 'sneak']);
    expect(options[0].getAttribute('title')).toBe('Also in Quilltap General, which this one outranks');
    expect(options[1].hasAttribute('title')).toBe(false);
  });

  it('reads an absent shadows / stores / unread as empty', async () => {
    const { fixture } = await mount(
      stub({ pool: { presets: [{ ...GROUP, name: 'brute', path: 'Tools/unlock.brute.settings.json' }] } }),
    );
    const options = Array.from(loadSelect(fixture)!.options).slice(1);
    expect(options[0].hasAttribute('title')).toBe(false);
    expect(storeSelect(fixture)).toBeNull();
  });

  it('offers "No presets yet" on an empty walk, with the load select disabled', async () => {
    const { fixture } = await mount(stub({ pool: { presets: [], stores: [VAULT], unread: [] } }));
    expect(optionText(loadSelect(fixture)!.options[0])).toBe('No presets yet');
    expect(loadSelect(fixture)!.disabled).toBe(true);
  });

  it('offers "Consulting the stores…" while the walk is out', async () => {
    const s = stub();
    s.client.dispatchData = (() => new Promise(() => undefined)) as unknown as CoreClient['dispatchData'];
    const { fixture } = await mount(s);
    expect(optionText(loadSelect(fixture)!.options[0])).toBe('Consulting the stores…');
  });

  it('offers "Load a preset…" once there is one', async () => {
    const { fixture } = await mount(stub());
    expect(optionText(loadSelect(fixture)!.options[0])).toBe('Load a preset…');
    expect(loadSelect(fixture)!.disabled).toBe(false);
  });

  it("says so, inline, when the stores would not list (the server's sentence)", async () => {
    const { fixture } = await mount(
      stub({ fail: { chatCustomToolPresets: new CoreDispatchError({ kind: 'not-found', message: 'Chat not found' }) } }),
    );
    expect(el(fixture).querySelector('p.text-xs.qt-text-destructive')!.textContent?.trim()).toBe('Chat not found');
  });

  it("an Error's own message shows (even empty); a non-Error is v4's fallback sentence", async () => {
    const { fixture } = await mount(stub({ fail: { chatCustomToolPresets: new Error('') } }));
    expect(el(fixture).querySelector('p.text-xs.qt-text-destructive')!.textContent?.trim()).toBe('');
    // coreErrorMessage reads an Error's message as is (the v5 convention); a non-Error is the fallback.
    const s = stub();
    s.client.dispatchData = (async () => {
      throw 'nope';
    }) as unknown as CoreClient['dispatchData'];
    const second = await mount(s);
    expect(el(second.fixture).querySelector('p.text-xs.qt-text-destructive')!.textContent?.trim()).toBe(
      'The stores would not list their presets.',
    );
  });

  it('names the stores it could not look in', async () => {
    const { fixture } = await mount(
      stub({
        pool: { ...POOL, unread: [{ ...GROUP, reason: 'x' }, { ...GENERAL, reason: 'y' }] },
      }),
    );
    expect(text(fixture)).toContain(
      'Could not look in The Ring, Quilltap General — any presets kept there are missing from this list.',
    );
  });

  it('says nothing of unread stores when every store was read', async () => {
    const { fixture } = await mount(stub());
    expect(text(fixture)).not.toContain('Could not look in');
  });
});

describe('run presets — where a save goes', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers "Save into" over every store, defaulting to the vault', async () => {
    const { fixture } = await mount(stub());
    const select = storeSelect(fixture)!;
    expect(el(fixture).querySelector('label[for="custom-tool-unlock::char-1-preset-store"]')!.textContent?.trim()).toBe(
      'Save into',
    );
    expect(Array.from(select.options).map(optionText)).toEqual(['Charlie Vault', 'The Ring', 'Quilltap General']);
    expect(Array.from(select.options).map((o) => o.value)).toEqual(['vault-charlie', 'group-1', 'general']);
    expect(select.value).toBe('vault-charlie');
  });

  it('defaults to the nearest store when the vault is not on the walk', async () => {
    const { fixture } = await mount(stub(), (h) => h.vault.set('elsewhere'));
    expect(storeSelect(fixture)!.value).toBe('vault-charlie');
    const second = await mount(stub({ pool: { ...POOL, stores: [GROUP, GENERAL] } }));
    expect(storeSelect(second.fixture)!.value).toBe('group-1');
  });

  it('hides "Save into" with no stores, and refuses to save', async () => {
    const { fixture } = await mount(stub({ pool: { presets: [], stores: [], unread: [] } }));
    expect(storeSelect(fixture)).toBeNull();
    await typeName(fixture, 'brute');
    expect(button(fixture, 'Save preset').disabled).toBe(true);
  });

  it('refuses to save until the typed name passes the pattern', async () => {
    const { fixture } = await mount(stub());
    expect(button(fixture, 'Save preset').disabled).toBe(true);
    await typeName(fixture, 'brute');
    expect(button(fixture, 'Save preset').disabled).toBe(false);
  });

  it("files the COERCED values into the vault by default, with v4's toast naming the store", async () => {
    const s = stub();
    const { fixture } = await mount(s, (h) => h.values.set({ bonus: '3', quiet: true }));
    await typeName(fixture, 'brute');
    button(fixture, 'Save preset').click();
    await settle(fixture);
    expect(s.calls.find((c) => c['type'] === 'mountFileWrite')).toEqual({
      type: 'mountFileWrite',
      mountPointId: 'vault-charlie',
      path: 'Tools/unlock.brute.settings.json',
      content: JSON.stringify({ bonus: 3, quiet: true }, null, 2),
    });
    expect(toasts()).toEqual([{ type: 'success', message: 'Preset “brute” filed in Charlie Vault.' }]);
  });

  it('files into the PICKED store when the operator chooses one', async () => {
    const s = stub();
    const { fixture } = await mount(s);
    await choose(fixture, storeSelect(fixture)!, 'general');
    await typeName(fixture, 'brute');
    button(fixture, 'Save preset').click();
    await settle(fixture);
    expect(s.calls.find((c) => c['type'] === 'mountFileWrite')!['mountPointId']).toBe('general');
    expect(toasts()).toEqual([{ type: 'success', message: 'Preset “brute” filed in Quilltap General.' }]);
  });

  it('re-walks after a save, so the new preset is listed', async () => {
    const s = stub();
    const { fixture } = await mount(s);
    await typeName(fixture, 'brute');
    button(fixture, 'Save preset').click();
    await settle(fixture);
    expect(s.calls.filter((c) => c['type'] === 'chatCustomToolPresets')).toHaveLength(2);
  });

  it('names the failure when the write is refused', async () => {
    const { fixture } = await mount(
      stub({ fail: { mountFileWrite: new CoreDispatchError({ kind: 'internal', message: 'disk full' }) } }),
    );
    await typeName(fixture, 'brute');
    button(fixture, 'Save preset').click();
    await settle(fixture);
    expect(toasts()).toEqual([{ type: 'error', message: 'disk full' }]);
  });
});

describe('run presets — dealing one back', () => {
  afterEach(() => TestBed.resetTestingModule());

  const READS = {
    'group-1:Tools/unlock.brute.settings.json': JSON.stringify({ bonus: 5, quiet: true, gone: 1, nested: {} }),
    'vault-charlie:Tools/unlock.sneak.settings.json': JSON.stringify({ quiet: true }),
  };

  it("reads the preset from its OWN store and path, keyed by name", async () => {
    const s = stub({ reads: READS });
    const { fixture } = await mount(s);
    await choose(fixture, loadSelect(fixture)!, 'brute');
    expect(s.calls.find((c) => c['type'] === 'mountFileRead')).toEqual({
      type: 'mountFileRead',
      mountPointId: 'group-1',
      path: 'Tools/unlock.brute.settings.json',
    });
    expect(fixture.componentInstance.values()).toEqual({ bonus: '5', quiet: true });
    expect(loadSelect(fixture)!.value).toBe('');
  });

  it('copies the loaded name into the input, so re-saving updates it', async () => {
    const { fixture } = await mount(stub({ reads: READS }));
    await choose(fixture, loadSelect(fixture)!, 'brute');
    expect(nameInput(fixture).value).toBe('brute');
  });

  it('a load does not move the save store', async () => {
    const { fixture } = await mount(stub({ reads: READS }));
    await choose(fixture, loadSelect(fixture)!, 'brute');
    expect(storeSelect(fixture)!.value).toBe('vault-charlie');
  });

  it('leaves a parameter the preset does not mention at its current value', async () => {
    const { fixture } = await mount(stub({ reads: READS }), (h) => h.values.set({ bonus: '2', quiet: false }));
    await choose(fixture, loadSelect(fixture)!, 'sneak');
    expect(fixture.componentInstance.values()).toEqual({ bonus: '2', quiet: true });
  });

  it('refuses a file that would not read as JSON, naming its store', async () => {
    const { fixture } = await mount(stub({ reads: { 'group-1:Tools/unlock.brute.settings.json': '{not json' } }));
    await choose(fixture, loadSelect(fixture)!, 'brute');
    expect(toasts()).toEqual([
      { type: 'error', message: 'Preset “brute” would not read as JSON. It stays as written in The Ring.' },
    ]);
    expect(fixture.componentInstance.values()).toEqual({ bonus: '0', quiet: false });
  });

  it('refuses a file that parses but is not a settings object, naming its store', async () => {
    const { fixture } = await mount(stub({ reads: { 'group-1:Tools/unlock.brute.settings.json': '[1,2]' } }));
    await choose(fixture, loadSelect(fixture)!, 'brute');
    expect(toasts()).toEqual([
      { type: 'error', message: 'Preset “brute” is not a settings object. It stays as written in The Ring.' },
    ]);
  });

  it('names the failure when the read is refused', async () => {
    const { fixture } = await mount(
      stub({ fail: { mountFileRead: new CoreDispatchError({ kind: 'not-found', message: 'File not found' }) } }),
    );
    await choose(fixture, loadSelect(fixture)!, 'brute');
    expect(toasts()).toEqual([{ type: 'error', message: 'File not found' }]);
  });

  it('Reset to defaults re-seeds the form from the declared defaults', async () => {
    const { fixture } = await mount(stub(), (h) => h.values.set({ bonus: '9', quiet: true }));
    button(fixture, 'Reset to defaults').click();
    await settle(fixture);
    expect(fixture.componentInstance.values()).toEqual({ bonus: '0', quiet: false });
  });

  it('is inert while a run is in flight', async () => {
    const { fixture } = await mount(stub(), (h) => h.disabled.set(true));
    expect(loadSelect(fixture)!.disabled).toBe(true);
    expect(nameInput(fixture).disabled).toBe(true);
    expect(storeSelect(fixture)!.disabled).toBe(true);
    expect(button(fixture, 'Reset to defaults').disabled).toBe(true);
  });
});
