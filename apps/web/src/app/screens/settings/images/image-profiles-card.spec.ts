import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ImageProfileDto } from '../../../core/core-contract';
import { ImageProfilesCard } from './image-profiles-card';

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

function stubClient(profiles: ImageProfileDto[]): Partial<CoreClient> {
  return {
    dispatchData: (async () => ({ profiles })) as unknown as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<ImageProfilesCard>> {
  TestBed.configureTestingModule({
    imports: [ImageProfilesCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ImageProfilesCard);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('ImageProfilesCard', () => {
  it('renders the profile rows with the Default badge and API key label', async () => {
    const fixture = await render(
      stubClient([
        profile({ id: 'a', name: 'HD', isDefault: true }),
        profile({ id: 'b', name: 'Standard' }),
      ]),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Image Generation Profiles (2)');
    expect(text).toContain('HD');
    expect(text).toContain('Default');
    expect(text).toContain('My Key');
  });

  it('sorts the cards alphabetically by name (v4 card order)', async () => {
    const fixture = await render(
      stubClient([
        profile({ id: 'z', name: 'Zephyr' }),
        profile({ id: 'a', name: 'Aurora' }),
      ]),
    );
    const titles = Array.from(fixture.nativeElement.querySelectorAll('h3')).map((h) =>
      (h as HTMLElement).textContent?.trim(),
    );
    expect(titles).toEqual(['Aurora', 'Zephyr']);
  });

  it('marks a profile with no API key', async () => {
    const fixture = await render(stubClient([profile({ id: 'a', name: 'Keyless', apiKey: null })]));
    expect(fixture.nativeElement.textContent).toContain('⚠️ No API Key');
  });

  it('shows the empty state with v4 copy', async () => {
    const fixture = await render(stubClient([]));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('No image profiles yet');
    expect(text).toContain('Create a profile to start generating images with AI');
  });
});
