// v5-authored from v4 `components/settings/connection-profiles/ProfileCard.tsx:161-169`
// + `ProfileList.tsx:69-72` (`544c7ab1e`, at the pin `624d0d25c`) — v4 ships the
// card line without a test. P4.D388 Tier 1 item 6 / Tier 2 item 18.
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ConnectionProfileDto, CoreRequest, ProviderInfo } from '../../../core/core-contract';
import { ConnectionProfilesCard } from './connection-profiles-card';

function provider(name: string, samplingParameters?: boolean): ProviderInfo {
  return {
    id: name,
    name,
    displayName: name,
    description: '',
    abbreviation: name.slice(0, 2),
    type: 'llm',
    capabilities: {
      chat: true,
      imageGeneration: false,
      embeddings: false,
      webSearch: false,
      ...(samplingParameters === undefined ? {} : { samplingParameters }),
    } as ProviderInfo['capabilities'],
    configRequirements: { requiresApiKey: false, requiresBaseUrl: false },
  };
}

function profile(id: string, providerName: string, parameters: Record<string, unknown>): ConnectionProfileDto {
  return { id, name: `P-${id}`, provider: providerName, modelName: 'm', parameters, isDefault: false, sortIndex: Number(id) };
}

async function render(profiles: ConnectionProfileDto[], providers: ProviderInfo[]): Promise<HTMLElement> {
  const dispatchExpect = vi.fn(async (req: CoreRequest) => {
    switch (req.type) {
      case 'connectionProfileList':
        return { type: 'connectionProfiles', data: { profiles } };
      case 'providerList':
        return { type: 'providers', data: { providers } };
      default:
        return { type: 'apiKeys', data: { apiKeys: [] } };
    }
  });
  TestBed.configureTestingModule({
    imports: [ConnectionProfilesCard],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: { dispatchExpect, dispatchData: vi.fn(async () => ({})) } },
    ],
  });
  const fixture = TestBed.createComponent(ConnectionProfilesCard);
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture.nativeElement as HTMLElement;
}

const lines = (el: HTMLElement): string[] =>
  Array.from(el.querySelectorAll('div.qt-text-xs.mt-2')).map((d) =>
    (d.textContent ?? '').replace(/\s+/g, ' ').trim(),
  );

describe('ConnectionProfilesCard — the parameters line (v4 544c7ab1e ProfileCard)', () => {
  it('a provider that takes sampling shows all three, with the 0.7 / 1000 / 1 defaults', async () => {
    const el = await render(
      [profile('1', 'OPENAI', { temperature: 0.4, max_tokens: 2048, top_p: 0.9 }), profile('2', 'OPENAI', {})],
      [provider('OPENAI')],
    );
    expect(lines(el)).toEqual([
      'Temperature: 0.4 • Max Tokens: 2048 • Top P: 0.9',
      'Temperature: 0.7 • Max Tokens: 1000 • Top P: 1',
    ]);
  });

  it('a provider that declines sampling shows "Max Tokens" alone', async () => {
    const el = await render(
      [profile('1', 'ANTHROPIC', { temperature: 0.4, max_tokens: 2048, top_p: 0.9 }), profile('2', 'ANTHROPIC', {})],
      [provider('ANTHROPIC', false)],
    );
    expect(lines(el)).toEqual(['Max Tokens: 2048', 'Max Tokens: 1000']);
  });

  it('a provider missing from the listing (a removed plugin) shows all three (v4 `?? true`)', async () => {
    const el = await render([profile('1', 'GONE', { max_tokens: 5 })], [provider('ANTHROPIC', false)]);
    expect(lines(el)).toEqual(['Temperature: 0.7 • Max Tokens: 5 • Top P: 1']);
  });
});
