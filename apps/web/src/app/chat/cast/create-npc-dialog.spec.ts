import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';

import { CreateNpcDialog } from './create-npc-dialog';

function profile(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: 'prof-1',
    name: 'Everyday',
    provider: 'anthropic',
    modelName: 'claude-sonnet',
    parameters: {},
    isDefault: false,
    ...over,
  };
}

interface Stub {
  calls: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

function stub(opts: {
  profiles?: Array<Record<string, unknown>>;
  create?: Record<string, unknown> | Error;
  avatar?: Record<string, unknown> | Error;
}): Stub {
  const calls: Record<string, unknown>[] = [];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    switch (req['type']) {
      case 'connectionProfileList':
        return { profiles: opts.profiles ?? [profile()] };
      case 'characterCreate':
        if (opts.create instanceof Error) throw opts.create;
        return opts.create ?? { character: { id: 'npc-1' } };
      case 'characterAvatar':
        if (opts.avatar instanceof Error) throw opts.avatar;
        return opts.avatar ?? { success: true };
      default:
        return {};
    }
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(s: Stub): Promise<ComponentFixture<CreateNpcDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CreateNpcDialog],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(CreateNpcDialog);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

/** The dialog's protected form signals, reached the way the other dialog specs do. */
function form(fixture: ComponentFixture<CreateNpcDialog>): {
  name: { set(v: string): void };
  description: { set(v: string): void };
  physicalDescription: { set(v: string): void };
  scenario: { set(v: string): void };
  systemPrompt: { set(v: string): void };
  avatarFile: { set(v: File | null): void };
} {
  return fixture.componentInstance as unknown as ReturnType<typeof form>;
}

function createCall(s: Stub): Record<string, unknown> | undefined {
  return s.calls.find((c) => c['type'] === 'characterCreate');
}

function createdCharacter(s: Stub): Record<string, unknown> {
  return createCall(s)!['character'] as Record<string, unknown>;
}

function footerPrimary(fixture: ComponentFixture<unknown>): HTMLButtonElement {
  return fixture.nativeElement.querySelector(
    '.qt-dialog-footer .qt-button-primary',
  ) as HTMLButtonElement;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
}

describe('CreateNpcDialog (v4 components/chat/CreateNPCDialog.tsx)', () => {
  beforeEach(() => {
    // v4 mints ids for the nested records; jsdom has randomUUID, but pin it so
    // the asserted bodies are stable.
    let n = 0;
    vi.spyOn(crypto, 'randomUUID').mockImplementation(
      () => `00000000-0000-4000-8000-00000000000${++n}` as `${string}-${string}-${string}-${string}-${string}`,
    );
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  it('auto-selects the first connection profile once the list arrives (v4 :60-67)', async () => {
    const s = stub({ profiles: [profile({ id: 'prof-a' }), profile({ id: 'prof-b' })] });
    const fixture = await mount(s);
    form(fixture).name.set('Barkeep');
    form(fixture).description.set('Wipes the same glass forever.');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(createdCharacter(s)['defaultConnectionProfileId']).toBe('prof-a');
  });

  it('keeps Create disabled until name, description and a profile are all present', async () => {
    const s = stub({});
    const fixture = await mount(s);
    expect(footerPrimary(fixture).disabled).toBe(true);
    form(fixture).name.set('Barkeep');
    fixture.detectChanges();
    expect(footerPrimary(fixture).disabled).toBe(true);
    form(fixture).description.set('Wipes the same glass forever.');
    fixture.detectChanges();
    expect(footerPrimary(fixture).disabled).toBe(false);
  });

  it('flags the character `npc` and COPIES description into personality (v4 :167)', async () => {
    const s = stub({});
    const fixture = await mount(s);
    form(fixture).name.set('  Barkeep  ');
    form(fixture).description.set('  Wipes the same glass forever.  ');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(createdCharacter(s)).toEqual({
      npc: true,
      name: 'Barkeep',
      description: 'Wipes the same glass forever.',
      personality: 'Wipes the same glass forever.',
      defaultConnectionProfileId: 'prof-1',
    });
  });

  it('wraps a scenario in a one-element scenarios[] — never a scalar (v4 :172-176)', async () => {
    const s = stub({});
    const fixture = await mount(s);
    form(fixture).name.set('Barkeep');
    form(fixture).description.set('d');
    form(fixture).scenario.set('  The tavern at closing time.  ');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    const character = createdCharacter(s);
    expect('scenario' in character).toBe(false);
    expect(character['scenarios']).toEqual([
      { id: expect.any(String), title: 'Default', content: 'The tavern at closing time.' },
    ]);
  });

  it('sends the SINGULAR physicalDescription object — never the plural (v4 :191-203)', async () => {
    const s = stub({});
    const fixture = await mount(s);
    form(fixture).name.set('Barkeep');
    form(fixture).description.set('d');
    form(fixture).physicalDescription.set('Broad, grey, unhurried.');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    const character = createdCharacter(s);
    expect('physicalDescriptions' in character).toBe(false);
    expect(character['physicalDescription']).toMatchObject({
      name: 'Default',
      fullDescription: 'Broad, grey, unhurried.',
    });
  });

  it('sends a named default system prompt when one was written', async () => {
    const s = stub({});
    const fixture = await mount(s);
    form(fixture).name.set('Barkeep');
    form(fixture).description.set('d');
    form(fixture).systemPrompt.set('Speak in short sentences.');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(createdCharacter(s)['systemPrompts']).toEqual([
      {
        id: expect.any(String),
        name: 'Default',
        content: 'Speak in short sentences.',
        isDefault: true,
        createdAt: expect.any(String),
        updatedAt: expect.any(String),
      },
    ]);
  });

  it('omits every optional block when its field is blank', async () => {
    const s = stub({});
    const fixture = await mount(s);
    form(fixture).name.set('Barkeep');
    form(fixture).description.set('d');
    form(fixture).scenario.set('   ');
    form(fixture).systemPrompt.set('');
    form(fixture).physicalDescription.set('  ');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    const character = createdCharacter(s);
    expect('scenarios' in character).toBe(false);
    expect('systemPrompts' in character).toBe(false);
    expect('physicalDescription' in character).toBe(false);
  });

  it('emits the new id and closes', async () => {
    const s = stub({ create: { character: { id: 'npc-42' } } });
    const fixture = await mount(s);
    const created: string[] = [];
    let closed = 0;
    fixture.componentInstance.created.subscribe((id) => created.push(id));
    fixture.componentInstance.close.subscribe(() => (closed += 1));
    form(fixture).name.set('Barkeep');
    form(fixture).description.set('d');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(created).toEqual(['npc-42']);
    expect(closed).toBe(1);
  });

  it('uploads the avatar into the new character’s gallery, then pins the linkId', async () => {
    // The upload leg is a real multipart `fetch` (the ported photo route), so the
    // transport is stubbed rather than the helper — that keeps the assertion on
    // the request v5 actually makes.
    const upload = vi.fn(async () => ({
      ok: true,
      status: 200,
      json: async () => ({
        linkId: 'link-9',
        mountPointId: 'm',
        relativePath: 'p',
        keptAt: 'k',
        sha256: 's',
      }),
    }));
    vi.stubGlobal('fetch', upload as unknown as typeof fetch);
    const s = stub({ create: { character: { id: 'npc-7' } } });
    const fixture = await mount(s);
    form(fixture).name.set('Barkeep');
    form(fixture).description.set('d');
    form(fixture).avatarFile.set(new File(['x'], 'face.png', { type: 'image/png' }));
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);

    // The v5 sequence: create → upload into the gallery → pin. (v4 uploads
    // first, to a generic images route v5 does not carry.)
    expect(upload).toHaveBeenCalledTimes(1);
    const url = String((upload.mock.calls as unknown as unknown[][])[0]![0]);
    expect(url).toContain('/api/v1/characters/npc-7/photos');
    expect(s.calls.find((c) => c['type'] === 'characterAvatar')).toEqual({
      type: 'characterAvatar',
      characterId: 'npc-7',
      imageId: 'link-9',
    });
  });

  it('an avatar failure does NOT fail the creation (v4 :236-241)', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({
        ok: false,
        status: 507,
        json: async () => ({ error: 'disk full' }),
      })) as unknown as typeof fetch,
    );
    const s = stub({ create: { character: { id: 'npc-7' } } });
    const fixture = await mount(s);
    const created: string[] = [];
    fixture.componentInstance.created.subscribe((id) => created.push(id));
    form(fixture).name.set('Barkeep');
    form(fixture).description.set('d');
    form(fixture).avatarFile.set(new File(['x'], 'face.png', { type: 'image/png' }));
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(created).toEqual(['npc-7']);
    expect(text(fixture)).not.toContain('disk full');
  });

  it('shows the server’s message and stays open when creation fails', async () => {
    const s = stub({ create: new Error('A character named Barkeep already exists') });
    const fixture = await mount(s);
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => (closed += 1));
    form(fixture).name.set('Barkeep');
    form(fixture).description.set('d');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(text(fixture)).toContain('A character named Barkeep already exists');
    expect(closed).toBe(0);
  });

  it('rejects a non-image avatar file (v4 :110-114)', async () => {
    const s = stub({});
    const fixture = await mount(s);
    const input = fixture.nativeElement.querySelector(
      'input[type="file"]',
    ) as HTMLInputElement;
    const file = new File(['x'], 'notes.txt', { type: 'text/plain' });
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    input.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(text(fixture)).toContain('Please select an image file');
    expect(text(fixture)).toContain('No image selected');
  });

  it('warns when the instance has no connection profile at all', async () => {
    const s = stub({ profiles: [] });
    const fixture = await mount(s);
    expect(text(fixture)).toContain('No connection profiles available');
  });
});
