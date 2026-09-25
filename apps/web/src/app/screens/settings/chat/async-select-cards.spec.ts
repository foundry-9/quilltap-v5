import type { ComponentFixture } from '@angular/core/testing';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto } from '../../../core/core-contract';
import { settingsRow, settle } from './chat-settings.spec-harness';
import { ImageDescriptionSettings } from './image-description-settings';

/**
 * The async-options card (P4.6an unit 6; its Dangerous Content sibling left for
 * the Concierge tab with v4 #76). Their headline assertion is the
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
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: s.client },
    ],
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

  /**
   * v4 #76 (`3b463d6b1`): the uncensored fallback left this card for the
   * Concierge's vision profile, and the server now answers 400 to ANY PUT
   * carrying `uncensoredImageDescriptionProfileId`. The card offers a link in
   * the picker's place and never sends the key.
   */
  it('has no uncensored fallback picker — a link to the Concierge\'s desk instead (v4 #76)', async () => {
    const s = stub(settingsRow({ uncensoredImageDescriptionProfileId: 'p-vision' }), [VISION]);
    const fixture = await mount(ImageDescriptionSettings, s);
    const root = fixture.nativeElement as HTMLElement;
    expect(root.querySelector('#image-desc-fallback')).toBeNull();
    expect(root.querySelectorAll('select')).toHaveLength(1);
    const note = Array.from(root.querySelectorAll('p.qt-text-xs')).find((p) =>
      (p.textContent ?? '').includes('uncensored fallback'),
    );
    expect((note?.textContent ?? '').replace(/\s+/g, ' ').trim()).toBe(
      'The uncensored fallback, for when this profile refuses to describe an image, now keeps company with the Concierge: see its vision profile under The Concierge → The Uncensored Desk.',
    );
    const link = note?.querySelector('a.qt-link');
    expect(link?.getAttribute('href')).toBe('/settings?tab=concierge&section=uncensored-desk');
  });

  it('carries #76\'s subtitle', async () => {
    const s = stub(settingsRow(), [VISION]);
    const fixture = await mount(ImageDescriptionSettings, s);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      "When you attach an image to a chat with a provider that doesn't support images (like Ollama, OpenRouter, etc.), this profile describes it in text.",
    );
  });

  it('PUTs the primary alone — the body never carries the retired uncensored key (P4.D230)', async () => {
    const s = stub(settingsRow({ uncensoredImageDescriptionProfileId: 'p-vision' }), [VISION]);
    const fixture = await mount(ImageDescriptionSettings, s);
    await choose(fixture, select(fixture, 'image-desc-primary'), 'p-vision');
    expect(s.updates.map((u) => Object.keys(u))).toEqual([['imageDescriptionProfileId']]);
  });
});

// The Dangerous Content card's blocks left with the card (v4 #76, `3b463d6b1`):
// its controls moved to the Concierge tab, whose specs live under
// `screens/settings/concierge/`.
