import type { ComponentFixture } from '@angular/core/testing';
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto } from '../../../core/core-contract';
import { checkboxes, settingsRow, settle } from './chat-settings.spec-harness';
import { DangerousContentSettings } from './dangerous-content-settings';
import { ImageDescriptionSettings } from './image-description-settings';

/**
 * The two async-options cards (P4.6an unit 6). Their headline assertion is the
 * dogfood-#6 late-options regression: the stored profile id must still be
 * DISPLAYED on the render that lands before the profile list does. A `[value]`
 * on the select would blank it — hence the BINDING `[selected]`-per-option rule.
 */

interface Profile {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  supportsImageUpload?: boolean;
  isDangerousCompatible?: boolean;
  apiKey?: unknown;
}

function profile(over: Partial<Profile> & { id: string; name: string }): Profile {
  return {
    provider: 'OPENAI',
    modelName: 'gpt-4o-mini',
    apiKey: { id: 'k1' },
    ...over,
  };
}

interface Stub {
  updates: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

/**
 * The card's stub. Both the settings row and the profile lists resolve
 * ASYNCHRONOUSLY, which is what makes the late-options assertions real: the
 * card's first render has no options at all, so a `[value]`-on-select binding
 * would blank the stored id and never recover (Angular won't re-run the binding
 * — the bound value never changed).
 */
function stub(row: ChatSettingsDto, profiles: Profile[], imageProfiles: Profile[] = []): Stub {
  const updates: Record<string, unknown>[] = [];
  let current = row;

  const dispatchExpect = vi.fn(
    async (req: { type: string; settings?: Record<string, unknown> }) => {
      if (req.type === 'chatSettingsUpdate') {
        updates.push(req.settings ?? {});
        current = { ...current, ...(req.settings ?? {}) } as ChatSettingsDto;
        return { type: 'chatSettings', data: current };
      }
      if (req.type === 'connectionProfileList') {
        return { type: 'connectionProfiles', data: { profiles, count: profiles.length } };
      }
      return { type: 'chatSettings', data: current };
    },
  );
  const dispatchData = vi.fn(async (req: { type: string }) => {
    if (req.type === 'imageProfileList') {
      return { profiles: imageProfiles };
    }
    return {};
  });

  return {
    updates,
    client: {
      dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'],
      dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
    },
  };
}

async function mount<T>(component: new (...args: never[]) => T, s: Stub): Promise<ComponentFixture<T>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [component as never],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(component as never) as ComponentFixture<T>;
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function select(fixture: ComponentFixture<unknown>, id: string): HTMLSelectElement {
  return (fixture.nativeElement as HTMLElement).querySelector(`#${id}`) as HTMLSelectElement;
}

async function choose(
  fixture: ComponentFixture<unknown>,
  el: HTMLSelectElement,
  value: string,
): Promise<void> {
  el.value = value;
  el.dispatchEvent(new Event('change'));
  await settle(fixture);
}

const VISION = profile({ id: 'p-vision', name: 'Haiku Vision', supportsImageUpload: true });
const BLIND = profile({ id: 'p-blind', name: 'Text Only', supportsImageUpload: false });
const NO_KEY = profile({
  id: 'p-nokey',
  name: 'Keyless',
  supportsImageUpload: true,
  apiKey: null,
});

describe('ImageDescriptionSettings', () => {
  it('offers only vision-capable profiles', async () => {
    const s = stub(settingsRow(), [VISION, BLIND]);
    const fixture = await mount(ImageDescriptionSettings, s);
    const values = Array.from(select(fixture, 'image-desc-primary').options).map((o) => o.value);
    expect(values).toEqual(['', 'p-vision']);
  });

  it('warns when no vision-capable profile exists', async () => {
    const s = stub(settingsRow(), [BLIND]);
    const fixture = await mount(ImageDescriptionSettings, s);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'No vision-capable profiles found',
    );
  });

  it('marks a keyless profile in the option label (v4 ⚠️ No API Key)', async () => {
    const s = stub(settingsRow(), [NO_KEY]);
    const fixture = await mount(ImageDescriptionSettings, s);
    const opt = select(fixture, 'image-desc-primary').options[1];
    expect(opt.textContent?.trim()).toBe('Keyless (OPENAI • gpt-4o-mini) ⚠️ No API Key');
  });

  it('displays the stored id even though the options load async (dogfood #6)', async () => {
    const s = stub(settingsRow({ imageDescriptionProfileId: 'p-vision' }), [VISION]);
    const fixture = await mount(ImageDescriptionSettings, s);
    expect(select(fixture, 'image-desc-primary').value).toBe('p-vision');
    expect(
      select(fixture, 'image-desc-primary').options[
        select(fixture, 'image-desc-primary').selectedIndex
      ].textContent?.trim(),
    ).toContain('Haiku Vision');
  });

  it('PUTs the primary id as a bare scalar', async () => {
    const s = stub(settingsRow(), [VISION]);
    const fixture = await mount(ImageDescriptionSettings, s);
    await choose(fixture, select(fixture, 'image-desc-primary'), 'p-vision');
    expect(s.updates).toEqual([{ imageDescriptionProfileId: 'p-vision' }]);
  });

  it('PUTs null when the primary is cleared to auto-select (v4 value || null)', async () => {
    const s = stub(settingsRow({ imageDescriptionProfileId: 'p-vision' }), [VISION]);
    const fixture = await mount(ImageDescriptionSettings, s);
    await choose(fixture, select(fixture, 'image-desc-primary'), '');
    expect(s.updates).toEqual([{ imageDescriptionProfileId: null }]);
  });

  it('PUTs the fallback id under its own key', async () => {
    const s = stub(settingsRow(), [VISION]);
    const fixture = await mount(ImageDescriptionSettings, s);
    await choose(fixture, select(fixture, 'image-desc-fallback'), 'p-vision');
    expect(s.updates).toEqual([{ uncensoredImageDescriptionProfileId: 'p-vision' }]);
  });
});

const DANGER_TEXT = profile({ id: 'p-danger', name: 'Uncensored', isDangerousCompatible: true });
const SAFE = profile({ id: 'p-safe', name: 'Polite', isDangerousCompatible: false });
const DANGER_IMG = profile({
  id: 'i-danger',
  name: 'Uncensored Imagery',
  provider: 'REPLICATE',
  isDangerousCompatible: true,
});

function dangerRow(over: Record<string, unknown> = {}, cheap: Record<string, unknown> = {}) {
  return settingsRow({
    dangerousContentSettings: {
      mode: 'OFF',
      threshold: 0.7,
      scanTextChat: true,
      scanImagePrompts: true,
      scanImageGeneration: false,
      displayMode: 'SHOW',
      showWarningBadges: true,
      ...over,
    },
    cheapLLMSettings: {
      strategy: 'PROVIDER_CHEAPEST',
      fallbackToLocal: true,
      embeddingProvider: 'OPENAI',
      ...cheap,
    },
  });
}

describe('DangerousContentSettings', () => {
  it('hides everything but the mode select while mode is OFF', async () => {
    const s = stub(dangerRow(), [DANGER_TEXT]);
    const fixture = await mount(DangerousContentSettings, s);
    expect(select(fixture, 'danger-mode')).toBeTruthy();
    expect(select(fixture, 'danger-display-mode')).toBeNull();
    // The Important Notes box shows in every mode.
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Important Notes');
  });

  it('reveals the scan + display controls once mode leaves OFF', async () => {
    const s = stub(dangerRow({ mode: 'DETECT_ONLY' }), [DANGER_TEXT]);
    const fixture = await mount(DangerousContentSettings, s);
    expect(select(fixture, 'danger-display-mode')).toBeTruthy();
    // ...but the uncensored pickers are AUTO_ROUTE-only.
    expect(select(fixture, 'danger-text-profile')).toBeNull();
  });

  it('reveals the uncensored pickers only in AUTO_ROUTE', async () => {
    const s = stub(dangerRow({ mode: 'AUTO_ROUTE' }), [DANGER_TEXT, SAFE], [DANGER_IMG]);
    const fixture = await mount(DangerousContentSettings, s);
    expect(select(fixture, 'danger-text-profile')).toBeTruthy();
    // Only dangerous-compatible profiles are offered.
    expect(Array.from(select(fixture, 'danger-text-profile').options).map((o) => o.value)).toEqual([
      '',
      'p-danger',
    ]);
    expect(Array.from(select(fixture, 'danger-image-profile').options).map((o) => o.value)).toEqual([
      '',
      'i-danger',
    ]);
  });

  it('PUTs the whole bag with the mode replaced', async () => {
    const s = stub(dangerRow(), [DANGER_TEXT]);
    const fixture = await mount(DangerousContentSettings, s);
    await choose(fixture, select(fixture, 'danger-mode'), 'AUTO_ROUTE');
    expect(s.updates).toEqual([
      {
        dangerousContentSettings: {
          mode: 'AUTO_ROUTE',
          threshold: 0.7,
          scanTextChat: true,
          scanImagePrompts: true,
          scanImageGeneration: false,
          displayMode: 'SHOW',
          showWarningBadges: true,
        },
      },
    ]);
  });

  it('sends an EXPLICIT null when an uncensored picker returns to auto-detect (the unit-1 case)', async () => {
    const s = stub(
      dangerRow({ mode: 'AUTO_ROUTE', uncensoredTextProfileId: 'p-danger' }),
      [DANGER_TEXT],
      [DANGER_IMG],
    );
    const fixture = await mount(DangerousContentSettings, s);
    await choose(fixture, select(fixture, 'danger-text-profile'), '');
    const bag = s.updates[0]['dangerousContentSettings'] as Record<string, unknown>;
    expect(bag['uncensoredTextProfileId']).toBeNull();
    expect('uncensoredTextProfileId' in bag).toBe(true);
  });

  it('displays a stored uncensored id despite the async profile list (dogfood #6)', async () => {
    const s = stub(
      dangerRow({ mode: 'AUTO_ROUTE', uncensoredTextProfileId: 'p-danger' }),
      [DANGER_TEXT],
      [DANGER_IMG],
    );
    const fixture = await mount(DangerousContentSettings, s);
    expect(select(fixture, 'danger-text-profile').value).toBe('p-danger');
  });

  it('sends the threshold as a NUMBER (v4 parseFloat)', async () => {
    const s = stub(dangerRow({ mode: 'DETECT_ONLY' }), [DANGER_TEXT]);
    const fixture = await mount(DangerousContentSettings, s);
    const range = (fixture.nativeElement as HTMLElement).querySelector(
      '#danger-threshold',
    ) as HTMLInputElement;
    range.value = '0.3';
    range.dispatchEvent(new Event('change'));
    await settle(fixture);
    const bag = s.updates[0]['dangerousContentSettings'] as Record<string, unknown>;
    expect(bag['threshold']).toBe(0.3);
  });

  it('shows the threshold to one decimal (v4 toFixed(1))', async () => {
    const s = stub(dangerRow({ mode: 'DETECT_ONLY', threshold: 1 }), [DANGER_TEXT]);
    const fixture = await mount(DangerousContentSettings, s);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Detection Threshold (1.0)',
    );
  });

  it('clears the custom prompt to an explicit null (the other unit-1 case)', async () => {
    const s = stub(
      dangerRow({ mode: 'DETECT_ONLY', customClassificationPrompt: 'be strict' }),
      [DANGER_TEXT],
    );
    const fixture = await mount(DangerousContentSettings, s);
    const area = (fixture.nativeElement as HTMLElement).querySelector(
      '#danger-custom-prompt',
    ) as HTMLTextAreaElement;
    expect(area.value).toBe('be strict');
    area.value = '';
    area.dispatchEvent(new Event('change'));
    await settle(fixture);
    const bag = s.updates[0]['dangerousContentSettings'] as Record<string, unknown>;
    expect(bag['customClassificationPrompt']).toBeNull();
  });

  it('writes the image-prompt picker into the CHEAP-LLM bag, not the danger bag', async () => {
    const s = stub(dangerRow({ mode: 'DETECT_ONLY' }), [DANGER_TEXT]);
    const fixture = await mount(DangerousContentSettings, s);
    await choose(fixture, select(fixture, 'danger-image-prompt-profile'), 'p-danger');
    expect(s.updates).toEqual([
      {
        cheapLLMSettings: {
          strategy: 'PROVIDER_CHEAPEST',
          fallbackToLocal: true,
          embeddingProvider: 'OPENAI',
          imagePromptProfileId: 'p-danger',
        },
      },
    ]);
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const s = stub(dangerRow(), [DANGER_TEXT]);
    // Make the update reject.
    const original = s.client.dispatchExpect!;
    s.client.dispatchExpect = (async (req: { type: string }) => {
      if (req.type === 'chatSettingsUpdate') throw new Error('boom');
      return original(req as never, 'chatSettings' as never);
    }) as unknown as CoreClient['dispatchExpect'];
    const fixture = await mount(DangerousContentSettings, s);
    await choose(fixture, select(fixture, 'danger-mode'), 'DETECT_ONLY');
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update dangerous content settings',
    );
  });

  it('toggles a scan checkbox through the whole bag', async () => {
    const s = stub(dangerRow({ mode: 'DETECT_ONLY' }), [DANGER_TEXT]);
    const fixture = await mount(DangerousContentSettings, s);
    // The scan toggles are the first three checkboxes in the revealed section.
    await (async () => {
      const box = checkboxes(fixture)[2];
      box.checked = true;
      box.dispatchEvent(new Event('change'));
      await settle(fixture);
    })();
    const bag = s.updates[0]['dangerousContentSettings'] as Record<string, unknown>;
    expect(bag['scanImageGeneration']).toBe(true);
    expect(bag['scanTextChat']).toBe(true);
  });
});
