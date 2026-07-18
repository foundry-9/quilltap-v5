import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ImageProfileDto } from '../core/core-contract';
import { ImageProfilePicker } from './image-profile-picker';
import { providerIconDefaults } from './provider-icon';

function profile(over: Partial<ImageProfileDto>): ImageProfileDto {
  return {
    id: 'p1',
    userId: 'u1',
    name: 'Profile',
    provider: 'OPENAI',
    apiKeyId: 'k1',
    baseUrl: null,
    modelName: 'dall-e-3',
    parameters: {},
    isDefault: false,
    isDangerousCompatible: false,
    tags: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    apiKey: { id: 'k1', label: 'My Key', provider: 'OPENAI', isActive: true },
    ...over,
  };
}

interface Dispatched {
  calls: Record<string, unknown>[];
}

function stubClient(
  profiles: ImageProfileDto[],
  recorder?: Dispatched,
  fail?: string,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Record<string, unknown>) => {
      recorder?.calls.push(req);
      if (fail) throw new Error(fail);
      return { profiles };
    }) as unknown as CoreClient['dispatchData'],
  };
}

async function render(
  client: Partial<CoreClient>,
  inputs: Record<string, unknown> = {},
): Promise<ComponentFixture<ImageProfilePicker>> {
  TestBed.configureTestingModule({
    imports: [ImageProfilePicker],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ImageProfilePicker);
  for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function options(fixture: ComponentFixture<ImageProfilePicker>): HTMLOptionElement[] {
  return Array.from(fixture.nativeElement.querySelectorAll('option'));
}

describe('ImageProfilePicker (v4 components/image-profiles/ImageProfilePicker.tsx)', () => {
  it('leads with the "No image generation" empty option (v4 :88)', async () => {
    const fixture = await render(stubClient([profile({})]));
    const opts = options(fixture);
    expect(opts[0].value).toBe('');
    expect(opts[0].textContent!.trim()).toBe('No image generation');
  });

  it('labels an option "{name} ({modelName})" (v4 :92-96)', async () => {
    const fixture = await render(stubClient([profile({ name: 'Painter', modelName: 'dall-e-3' })]));
    expect(options(fixture)[1].textContent!.trim()).toBe('Painter (dall-e-3)');
  });

  it('appends " [Default]" for the default profile (v4 :94)', async () => {
    const fixture = await render(stubClient([profile({ name: 'Painter', isDefault: true })]));
    expect(options(fixture)[1].textContent!.trim()).toBe('Painter (dall-e-3) [Default]');
  });

  it('appends the no-API-key warning when apiKey is absent (v4 :90,:95)', async () => {
    const fixture = await render(stubClient([profile({ name: 'Painter', apiKey: null })]));
    expect(options(fixture)[1].textContent!.trim()).toBe('Painter (dall-e-3) ⚠️ No API Key');
  });

  it('appends both flags in v4 order — [Default] then the key warning (v4 :94-95)', async () => {
    const fixture = await render(
      stubClient([profile({ name: 'Painter', isDefault: true, apiKey: null })]),
    );
    expect(options(fixture)[1].textContent!.trim()).toBe(
      'Painter (dall-e-3) [Default] ⚠️ No API Key',
    );
  });

  it('does NOT auto-select the default profile — the empty option stays selected (v4 :37,:83)', async () => {
    const fixture = await render(stubClient([profile({ id: 'd1', isDefault: true })]));
    const opts = options(fixture);
    expect(opts[0].selected).toBe(true);
    expect(opts[1].selected).toBe(false);
  });

  it('selects per option rather than binding the select (the dogfood-#6 async-options rule)', async () => {
    const fixture = await render(stubClient([profile({ id: 'p1' }), profile({ id: 'p2' })]), {
      value: 'p2',
    });
    const opts = options(fixture);
    expect(opts.find((o) => o.value === 'p2')!.selected).toBe(true);
    expect(fixture.nativeElement.querySelector('select').getAttribute('value')).toBeNull();
  });

  it('reports an empty selection as null, not "" (v4 :84)', async () => {
    const fixture = await render(stubClient([profile({})]), { value: 'p1' });
    const emitted: (string | null)[] = [];
    fixture.componentInstance.changed.subscribe((v) => emitted.push(v));
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    select.value = '';
    select.dispatchEvent(new Event('change'));
    expect(emitted).toEqual([null]);
  });

  it('shows the selected profile detail card with its model name (v4 :110-129)', async () => {
    const fixture = await render(
      stubClient([profile({ id: 'p1', name: 'Painter', modelName: 'dall-e-3' })]),
      { value: 'p1' },
    );
    const card = fixture.nativeElement.querySelector('.qt-border-info');
    expect(card).not.toBeNull();
    expect(card.textContent).toContain('Painter');
    expect(card.textContent).toContain('dall-e-3');
  });

  it('hides the detail card when nothing is selected (v4 :110)', async () => {
    const fixture = await render(stubClient([profile({ id: 'p1' })]));
    expect(fixture.nativeElement.querySelector('.qt-border-info')).toBeNull();
  });

  it('hides the detail card when the value matches no loaded profile (v4 :113-114)', async () => {
    const fixture = await render(stubClient([profile({ id: 'p1' })]), { value: 'ghost' });
    expect(fixture.nativeElement.querySelector('.qt-border-info')).toBeNull();
  });

  it('shows the empty-list hint when there are no profiles (v4 :103-107)', async () => {
    const fixture = await render(stubClient([]));
    expect(fixture.nativeElement.textContent).toContain('No image profiles available');
  });

  it('surfaces a failed read as the error line (v4 :61-63,:101)', async () => {
    const fixture = await render(stubClient([], undefined, 'nope'));
    const err = fixture.nativeElement.querySelector('.qt-text-destructive');
    expect(err.textContent).toContain('nope');
  });

  it('suppresses the empty-list hint while an error is showing (v4 :103 gates on !error)', async () => {
    const fixture = await render(stubClient([], undefined, 'nope'));
    expect(fixture.nativeElement.textContent).not.toContain('No image profiles available');
  });

  it('passes sortByCharacter through when given (v4 :49-51)', async () => {
    const rec: Dispatched = { calls: [] };
    await render(stubClient([profile({})], rec), { characterId: 'char-7' });
    expect(rec.calls.at(-1)).toEqual({ type: 'imageProfileList', sortByCharacter: 'char-7' });
  });

  it('omits sortByCharacter when absent, and never sends the deferred sortByUserCharacter', async () => {
    const rec: Dispatched = { calls: [] };
    await render(stubClient([profile({})], rec));
    expect(rec.calls.at(-1)).toEqual({ type: 'imageProfileList' });
  });
});

describe('providerIconDefaults (v4 ProviderIcon PROVIDER_DEFAULTS :52-62)', () => {
  it('maps a known provider to its abbreviation and colour', () => {
    expect(providerIconDefaults('OPENAI')).toEqual({ color: 'qt-text-success', abbrev: 'OAI' });
    expect(providerIconDefaults('GOOGLE_IMAGEN')).toEqual({ color: 'qt-text-info', abbrev: 'GGL' });
  });

  it('falls back to the first three characters upper-cased (v4 :170-173)', () => {
    expect(providerIconDefaults('mystery')).toEqual({
      color: 'qt-text-secondary',
      abbrev: 'MYS',
    });
  });
});
