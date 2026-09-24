import type { ComponentFixture } from '@angular/core/testing';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { BrahmaConsoleService } from '../../../brahma/brahma-console.service';
import { BrahmaConsoleApi } from '../../../brahma/brahma-wire';
import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto } from '../../../core/core-contract';
import { CharactersList } from '../../characters/list/characters-list';
import { settingsRow, settle } from './chat-settings.spec-harness';
import { DangerousContentSettings } from './dangerous-content-settings';

/**
 * ONE raw shape per connection-profiles cache entry (P4.116).
 *
 * v4 caches the raw `/api/v1/connection-profiles` envelope under ONE key,
 * `queryKeys.connectionProfiles.all = ['connection-profiles']`
 * (`lib/query/keys.ts:108-109` at `d1c06cd9d`), and a reader that wants a
 * narrower shape maps it with `select: mapProfiles` (`hooks/
 * useConnectionProfiles.ts:43-49`) — the mapping is never STORED. v5's readers
 * each ran their own `queryFn`, and two of them stored a MAPPED shape in the
 * shared entry: the Brahma console (`{id, name, provider, modelName}` — a
 * `providedIn: 'root'` observer that lives for the app) and the character
 * screens (`{…, isDefault}`). Whichever observer fetched last wrote the ONE
 * entry in its shape, so the Settings cards could read Brahma's flag-less rows
 * — `isDangerousCompatible` gone, the uncensored picker empty.
 *
 * The cache is the app's: a `staleTime` of 5 s (`app.config.ts`), shared by
 * every reader mounted against it.
 */

interface Row {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isDefault?: boolean;
  isDangerousCompatible?: boolean;
  supportsImageUpload?: boolean;
  allowToolUse?: boolean;
  apiKey?: unknown;
}

const DANGER: Row = {
  id: 'p-danger',
  name: 'Uncensored',
  provider: 'OPENROUTER',
  modelName: 'some/uncensored',
  isDefault: false,
  isDangerousCompatible: true,
  supportsImageUpload: false,
  allowToolUse: true,
  apiKey: { id: 'k1' },
};
const SAFE: Row = {
  id: 'p-safe',
  name: 'Polite',
  provider: 'ANTHROPIC',
  modelName: 'claude-sonnet-5',
  isDefault: true,
  isDangerousCompatible: false,
  supportsImageUpload: true,
  allowToolUse: false,
  apiKey: { id: 'k2' },
};
const ROWS = [DANGER, SAFE];

function autoRouteRow(): ChatSettingsDto {
  return settingsRow({
    dangerousContentSettings: {
      mode: 'AUTO_ROUTE',
      threshold: 0.7,
      scanTextChat: true,
      scanImagePrompts: true,
      scanImageGeneration: false,
      displayMode: 'SHOW',
      showWarningBadges: true,
    },
  });
}

/** One client answering every shape the readers ask for (both dispatch forms). */
function client(): { core: Partial<CoreClient>; profileFetches: () => number } {
  let fetches = 0;
  const row = autoRouteRow();
  const dispatchExpect = vi.fn(async (req: { type: string }) => {
    if (req.type === 'connectionProfileList') {
      fetches++;
      return { type: 'connectionProfiles', data: { profiles: ROWS, count: ROWS.length } };
    }
    return { type: 'chatSettings', data: row };
  });
  const dispatchData = vi.fn(async (req: { type: string }) => {
    if (req.type === 'connectionProfileList') {
      fetches++;
      return { profiles: ROWS, count: ROWS.length };
    }
    if (req.type === 'imageProfileList') return { profiles: [] };
    if (req.type === 'characterList') return { characters: [] };
    return {};
  });
  return {
    core: {
      dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'],
      dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
    },
    profileFetches: () => fetches,
  };
}

function configure(queryClient: QueryClient, core: Partial<CoreClient>): void {
  TestBed.configureTestingModule({
    providers: [
      provideRouter([]),
      provideTanStackQuery(queryClient),
      { provide: CoreClient, useValue: core },
      { provide: BrahmaConsoleApi, useValue: {} },
    ],
  });
}

function appQueryClient(): QueryClient {
  return new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 5_000 } } });
}

async function tick(): Promise<void> {
  for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
}

async function mountDangerCard(): Promise<ComponentFixture<DangerousContentSettings>> {
  const fixture = TestBed.createComponent(DangerousContentSettings);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function uncensoredOptions(fixture: ComponentFixture<unknown>): string[] {
  const select = (fixture.nativeElement as HTMLElement).querySelector(
    '#danger-text-profile',
  ) as HTMLSelectElement | null;
  return select ? Array.from(select.options).map((o) => o.value) : [];
}

afterEach(() => TestBed.resetTestingModule());

describe('the connection-profiles cache entry holds the RAW rows (P4.116)', () => {
  it('the Brahma console fetching first leaves the Settings card its isDangerousCompatible list', async () => {
    const qc = appQueryClient();
    const c = client();
    configure(qc, c.core);
    const brahma = TestBed.inject(BrahmaConsoleService);
    await tick();
    expect(c.profileFetches()).toBe(1);
    // Brahma's picker still gets its narrow shape — mapped on READ.
    expect(brahma.profiles()).toEqual(
      ROWS.map(({ id, name, provider, modelName }) => ({ id, name, provider, modelName })),
    );

    // Inside the 5 s window the card reads the entry Brahma filled, unfetched.
    const card = await mountDangerCard();
    expect(c.profileFetches()).toBe(1);
    expect(uncensoredOptions(card)).toEqual(['', 'p-danger']);
  });

  it('a refetch run by either observer keeps both readers whole', async () => {
    const qc = appQueryClient();
    const c = client();
    configure(qc, c.core);
    const card = await mountDangerCard();
    const brahma = TestBed.inject(BrahmaConsoleService);
    await tick();
    await qc.invalidateQueries({ queryKey: ['connection-profiles'] });
    await tick();
    card.detectChanges();
    await settle(card);
    expect(c.profileFetches()).toBeGreaterThanOrEqual(2);
    expect(uncensoredOptions(card)).toEqual(['', 'p-danger']);
    expect(brahma.profiles().map((p) => p.id)).toEqual(['p-danger', 'p-safe']);
  });

  it('a character screen fetching leaves the RAW rows in the entry, mapped only on read', async () => {
    const qc = appQueryClient();
    const c = client();
    configure(qc, c.core);
    const list = TestBed.createComponent(CharactersList);
    list.detectChanges();
    await settle(list);
    expect(c.profileFetches()).toBe(1);
    expect(qc.getQueryData(['connection-profiles'])).toEqual(ROWS);

    // One spelling (v4's): the Settings card mounted next reads the SAME entry.
    const card = await mountDangerCard();
    expect(c.profileFetches()).toBe(1);
    expect(uncensoredOptions(card)).toEqual(['', 'p-danger']);
  });
});
