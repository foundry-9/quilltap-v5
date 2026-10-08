import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto, ImageProfileDto } from '../../../core/core-contract';
import { WardrobeImagesCard } from './wardrobe-images-card';

/**
 * P4.D261 — v4 `7c8572869` + `b3f937076`
 * `__tests__/unit/components/settings/WardrobeImageSettings.test.tsx` (×7) and
 * `__tests__/unit/hooks/useChatSettings.test.tsx`'s two whole-bag assertions,
 * at the pin `f5e953a3f`, transcribed onto the `ChatSettingsCard` substrate
 * (the `story-backgrounds-card.spec.ts` harness). Markup and strings:
 * `WardrobeImageSettings.tsx:28-102`; the handlers `useChatSettings.ts:650-686`.
 */
const PROFILES = [
  {
    id: 'p-default',
    name: 'House Artist',
    provider: 'OPENAI',
    modelName: 'gpt-image-1',
    isDefault: true,
    isDangerousCompatible: false,
  },
  {
    id: 'p-wild',
    name: 'Back Room',
    provider: 'GROK',
    modelName: 'grok-image',
    isDefault: false,
    isDangerousCompatible: true,
  },
] as unknown as ImageProfileDto[];

interface Stub {
  updates: Record<string, unknown>[];
  client: Partial<CoreClient>;
  hold: () => () => void;
}

function stub(row: ChatSettingsDto, profiles: ImageProfileDto[] = PROFILES, failUpdate = false): Stub {
  const updates: Record<string, unknown>[] = [];
  let current = row;
  let gate: Promise<void> | null = null;
  const dispatchExpect = vi.fn(async (req: { type: string; settings?: Record<string, unknown> }) => {
    if (req.type === 'chatSettingsUpdate') {
      if (gate) await gate;
      if (failUpdate) throw new Error('boom');
      updates.push(req.settings ?? {});
      current = { ...current, ...(req.settings ?? {}) } as ChatSettingsDto;
    }
    return { type: 'chatSettings', data: current };
  });
  const dispatchData = vi.fn(async () => ({ profiles }) as unknown as Record<string, unknown>);
  return {
    updates,
    client: {
      dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'],
      dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
    },
    hold: () => {
      let release!: () => void;
      gate = new Promise((r) => (release = r));
      return () => {
        gate = null;
        release();
      };
    },
  };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(s: Stub): Promise<ComponentFixture<WardrobeImagesCard>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WardrobeImagesCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(WardrobeImagesCard);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

const row = (over: Record<string, unknown> = {}): ChatSettingsDto =>
  ({ avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR', ...over }) as ChatSettingsDto;
const el = (f: ComponentFixture<unknown>): HTMLElement => f.nativeElement as HTMLElement;
/** v4 `getByLabelText('Wardrobe Artist')`. */
const artist = (f: ComponentFixture<unknown>): HTMLSelectElement => {
  const label = [...el(f).querySelectorAll('label')].find(
    (l) => l.textContent!.trim() === 'Wardrobe Artist',
  )!;
  return el(f).querySelector<HTMLSelectElement>(`#${label.getAttribute('for')}`)!;
};
const toolsBox = (f: ComponentFixture<unknown>): HTMLInputElement =>
  el(f).querySelector<HTMLInputElement>('input[type="checkbox"]')!;
const change = async (f: ComponentFixture<unknown>, s: HTMLSelectElement, value: string) => {
  s.value = value;
  s.dispatchEvent(new Event('change'));
  await settle(f);
};

afterEach(() => TestBed.resetTestingModule());

describe('WardrobeImagesCard (v4 WardrobeImageSettings.test.tsx)', () => {
  it('shows the helper copy', async () => {
    const f = await mount(stub(row()));
    expect(el(f).textContent).toMatch(/Which artist draws the garments/);
    expect(el(f).textContent).toContain(
      "Which artist draws the garments. Pick a desk that will not balk at the odd corset; the Concierge's uncensored desk stands in if it does.",
    );
  });

  it('selects the default image profile when nothing is designated', async () => {
    const f = await mount(stub(row()));
    expect(artist(f).value).toBe('');
    expect(artist(f).options[0].textContent!.trim()).toBe('The default image profile (House Artist)');
  });

  it('lists every image profile and marks the uncensored ones', async () => {
    const f = await mount(stub(row()));
    expect([...artist(f).options].map((o) => o.textContent!.trim())).toEqual([
      'The default image profile (House Artist)',
      'House Artist (OPENAI - gpt-image-1)',
      'Back Room (GROK - grok-image) (uncensored)',
    ]);
  });

  it('reflects a stored designation', async () => {
    const f = await mount(stub(row({ wardrobeImageSettings: { imageProfileId: 'p-wild' } })));
    expect(artist(f).value).toBe('p-wild');
  });

  it('reports the chosen profile, and null for the default — the WHOLE bag, generateFromTools riding along', async () => {
    const s = stub(row({ wardrobeImageSettings: { imageProfileId: 'p-wild' } }));
    const f = await mount(s);
    await change(f, artist(f), 'p-default');
    await change(f, artist(f), '');
    // The card spreads EXACTLY the bag it read (v4 `{ ...currentSettings,
    // imageProfileId }`, useChatSettings.ts:660-664) — this fixture stores a
    // one-key bag, so one key travels; a real read always carries both (C2 §7).
    expect(s.updates).toEqual([
      { wardrobeImageSettings: { imageProfileId: 'p-default' } },
      { wardrobeImageSettings: { imageProfileId: null } },
    ]);
  });

  it('a row without the bag (a pre-round server) saves v4’s full default bag', async () => {
    const s = stub(row());
    const f = await mount(s);
    await change(f, artist(f), 'p-default');
    expect(s.updates).toEqual([
      { wardrobeImageSettings: { imageProfileId: 'p-default', generateFromTools: false } },
    ]);
  });

  it('is disabled while a save is in flight', async () => {
    const s = stub(row());
    const f = await mount(s);
    const release = s.hold();
    artist(f).value = 'p-wild';
    artist(f).dispatchEvent(new Event('change'));
    await settle(f);
    expect(artist(f).disabled).toBe(true);
    expect(toolsBox(f).disabled).toBe(true);
    release();
    await settle(f);
    expect(artist(f).disabled).toBe(false);
  });

  it('says so when no image profiles exist', async () => {
    const f = await mount(stub(row(), []));
    const warn = [...el(f).querySelectorAll('p.qt-text-warning')].map((p) => p.textContent!.trim());
    expect(warn).toEqual([
      'The studio stands empty: no image profiles have been engaged. Commission one in Image Profiles above before any garment can sit for its portrait.',
    ]);
    expect(artist(f).options[0].textContent!.trim()).toBe('The default image profile');
  });
});

describe('WardrobeImagesCard — Portraits from the Wardrobe Tools (v4 b3f937076)', () => {
  it('renders FIRST in the body with v4’s copy, off by default', async () => {
    const f = await mount(stub(row()));
    const box = toolsBox(f);
    expect(box.checked).toBe(false);
    expect(box.className).toBe('qt-checkbox mt-1');
    const label = box.closest('label')!;
    expect(label.className).toBe(
      'flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer',
    );
    expect(label.textContent!.replace(/\s+/g, ' ').trim()).toBe(
      "Portraits from the Wardrobe Tools When a character runs up a new garment with the wardrobe tools, or alters one's look, send it round to the artist for a portrait. Each sitting is a paid commission, which is why the door stays shut until you open it.",
    );
    // FIRST, before the artist picker.
    const body = label.closest('.space-y-6')!;
    expect(body.firstElementChild!.contains(label)).toBe(true);
  });

  it('the switch PUTs the whole bag, keeping the designated profile (useChatSettings.test.tsx:147-164)', async () => {
    const s = stub(row({ wardrobeImageSettings: { imageProfileId: 'profile-1', generateFromTools: false } }));
    const f = await mount(s);
    toolsBox(f).checked = true;
    toolsBox(f).dispatchEvent(new Event('change'));
    await settle(f);
    expect(s.updates).toEqual([
      { wardrobeImageSettings: { imageProfileId: 'profile-1', generateFromTools: true } },
    ]);
    // Assert the DOM after the save settles (the P4.D252 radio-resync lesson).
    expect(toolsBox(f).checked).toBe(true);
  });

  it('surfaces a failed save with v4’s sentence (the dogfood-#6 rule)', async () => {
    const s = stub(row(), PROFILES, true);
    const f = await mount(s);
    toolsBox(f).checked = true;
    toolsBox(f).dispatchEvent(new Event('change'));
    await settle(f);
    expect(el(f).querySelector('.qt-alert-error')!.textContent).toContain(
      'Failed to update wardrobe image settings',
    );
  });
});
