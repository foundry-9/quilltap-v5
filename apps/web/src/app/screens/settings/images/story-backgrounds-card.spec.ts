import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto, ImageProfileDto } from '../../../core/core-contract';
import { StoryBackgroundsCard } from './story-backgrounds-card';

function imageProfile(over: Partial<ImageProfileDto> = {}): ImageProfileDto {
  return {
    id: 'ip-1',
    name: 'Lantern',
    provider: 'openai',
    modelName: 'gpt-image-1',
    isDefault: false,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  } as ImageProfileDto;
}

interface Stub {
  updates: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

/**
 * The card reads/writes chat settings through `dispatchExpect` (the
 * ChatSettingsCard substrate) and lists image profiles through `dispatchData`.
 */
function stub(row: ChatSettingsDto, profiles: ImageProfileDto[], failUpdate = false): Stub {
  const updates: Record<string, unknown>[] = [];
  let current = row;

  const dispatchExpect = vi.fn(async (req: { type: string; settings?: Record<string, unknown> }) => {
    if (req.type === 'chatSettingsUpdate') {
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
  };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(s: Stub): Promise<ComponentFixture<StoryBackgroundsCard>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [StoryBackgroundsCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(StoryBackgroundsCard);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function settingsRow(over: Record<string, unknown> = {}): ChatSettingsDto {
  return { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR', ...over } as ChatSettingsDto;
}

function checkbox(fixture: ComponentFixture<unknown>): HTMLInputElement {
  return (fixture.nativeElement as HTMLElement).querySelector('input[type="checkbox"]')!;
}

function select(fixture: ComponentFixture<unknown>): HTMLSelectElement {
  return (fixture.nativeElement as HTMLElement).querySelector('select')!;
}

describe('StoryBackgroundsCard (v4 chat-settings/StoryBackgroundsSettings.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const bag = { enabled: true, defaultImageProfileId: 'ip-2' };

  it('reflects the stored bag: the toggle and the selected profile', async () => {
    const fixture = await mount(
      stub(settingsRow({ storyBackgroundsSettings: bag }), [
        imageProfile({ id: 'ip-1', name: 'Lantern' }),
        imageProfile({ id: 'ip-2', name: 'Aurora' }),
      ]),
    );
    expect(checkbox(fixture).checked).toBe(true);
    expect(select(fixture).value).toBe('ip-2');
  });

  it('defaults to disabled + no profile when the bag is absent (v4 Zod defaults)', async () => {
    const fixture = await mount(stub(settingsRow(), [imageProfile()]));
    expect(checkbox(fixture).checked).toBe(false);
    expect(select(fixture).value).toBe('');
  });

  it('labels options as "name (provider - modelName)" — v4 uses a HYPHEN', async () => {
    // Not a bullet: the sibling Chat-tab profile pickers use " • ", this one does
    // not. The copy is v4's, per picker.
    const fixture = await mount(
      stub(settingsRow(), [imageProfile({ name: 'Aurora', provider: 'openai', modelName: 'dall-e-3' })]),
    );
    const options = Array.from(select(fixture).options).map((o) => o.textContent!.trim());
    expect(options).toEqual(['Use default profile', 'Aurora (openai - dall-e-3)']);
  });

  describe('the whole-bag PUT rule', () => {
    it('PUTs the WHOLE bag with `enabled` replaced, preserving the profile id', async () => {
      const s = stub(settingsRow({ storyBackgroundsSettings: bag }), [imageProfile({ id: 'ip-2' })]);
      const fixture = await mount(s);

      checkbox(fixture).checked = false;
      checkbox(fixture).dispatchEvent(new Event('change'));
      await settle(fixture);

      // A partial nested patch would DROP defaultImageProfileId — the server
      // replaces the JSON column wholesale.
      expect(s.updates).toEqual([
        { storyBackgroundsSettings: { enabled: false, defaultImageProfileId: 'ip-2' } },
      ]);
    });

    it('PUTs the WHOLE bag with the profile replaced, preserving `enabled`', async () => {
      const s = stub(settingsRow({ storyBackgroundsSettings: bag }), [
        imageProfile({ id: 'ip-2' }),
        imageProfile({ id: 'ip-3' }),
      ]);
      const fixture = await mount(s);

      select(fixture).value = 'ip-3';
      select(fixture).dispatchEvent(new Event('change'));
      await settle(fixture);

      expect(s.updates).toEqual([
        { storyBackgroundsSettings: { enabled: true, defaultImageProfileId: 'ip-3' } },
      ]);
    });

    it('sends null (not "") when "Use default profile" is chosen — v4 `value || null`', async () => {
      const s = stub(settingsRow({ storyBackgroundsSettings: bag }), [imageProfile({ id: 'ip-2' })]);
      const fixture = await mount(s);

      select(fixture).value = '';
      select(fixture).dispatchEvent(new Event('change'));
      await settle(fixture);

      expect(s.updates).toEqual([
        { storyBackgroundsSettings: { enabled: true, defaultImageProfileId: null } },
      ]);
    });
  });

  it('disables the profile select while story backgrounds are off (v4 `!enabled`)', async () => {
    const fixture = await mount(
      stub(settingsRow({ storyBackgroundsSettings: { enabled: false, defaultImageProfileId: null } }), [
        imageProfile(),
      ]),
    );
    expect(select(fixture).disabled).toBe(true);
  });

  it('warns when no image profiles are configured', async () => {
    const fixture = await mount(stub(settingsRow(), []));
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'No image profiles configured.',
    );
  });

  it('surfaces a failed save (the dogfood-#6 rule — v4 only console.errors)', async () => {
    const s = stub(settingsRow({ storyBackgroundsSettings: bag }), [imageProfile()], true);
    const fixture = await mount(s);

    checkbox(fixture).checked = false;
    checkbox(fixture).dispatchEvent(new Event('change'));
    await settle(fixture);

    expect((fixture.nativeElement as HTMLElement).querySelector('.qt-alert-error')).not.toBeNull();
  });
});
