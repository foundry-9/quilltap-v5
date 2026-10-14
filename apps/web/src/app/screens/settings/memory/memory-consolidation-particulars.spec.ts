import { ChangeDetectionStrategy, Component } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import type { ConnectionProfileDto, MemoryConsolidationConfig } from '../../../core/core-contract';
import { fetchConsolidationConfig, memoryKeys, setConsolidationConfig } from '../../../memory/memory.api';
import { MemoryConsolidationParticulars } from './memory-consolidation-particulars';
import { MEMORY_SETTING_DEFAULTS } from './memory-setting-defaults';
import { injectMemorySettingsDocument } from './memory-settings-document';

/**
 * P4.D300 — the Consolidation particulars, v4 `components/tools/
 * memory-consolidation-particulars.tsx` (`b045dd986`, `4ae0c4ca6`) at
 * `83334b9a5`. v4 shipped no test for it; these rows are the lane's own. The
 * R-E / one-key idioms are transcribed from P4.D276's consolidation-card spec.
 */

@Component({
  selector: 'qt-particulars-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MemoryConsolidationParticulars],
  template: `<qt-memory-consolidation-particulars [doc]="doc" /><p id="err">{{ doc.error() }}</p>`,
})
class Host {
  readonly doc = injectMemorySettingsDocument<MemoryConsolidationConfig>({
    queryKey: memoryKeys.consolidationConfig(),
    read: fetchConsolidationConfig,
    write: setConsolidationConfig,
    defaults: MEMORY_SETTING_DEFAULTS.consolidation,
    noun: 'consolidation settings',
  });
}

interface Req {
  type: string;
  config?: Record<string, unknown>;
}
type Handler = (req: Req) => Record<string, unknown> | Promise<Record<string, unknown>>;

const STORED = { ...MEMORY_SETTING_DEFAULTS.consolidation, clusterThreshold: 0.9, connectionProfileId: 'p1' };

function profile(over: Partial<ConnectionProfileDto>): ConnectionProfileDto {
  return { id: 'p1', name: 'House', provider: 'OPENAI', modelName: 'gpt-4o', parameters: {}, isDefault: false, ...over };
}
const PROFILES = [profile({}), profile({ id: 'p2', name: 'Bare', provider: 'OLLAMA', modelName: '' })];

function render(overrides: Partial<Record<string, Handler>> = {}, seen: Req[] = []): ComponentFixture<Host> {
  const handlers: Record<string, Handler> = {
    memoryConsolidationConfigGet: () => ({ success: true, settings: { ...STORED } }),
    memoryConsolidationConfigSet: (req) => ({ success: true, settings: { ...STORED, ...req.config } }),
    ...overrides,
  };
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      const h = handlers[req.type];
      return h ? h(req) : {};
    }) as CoreClient['dispatchData'],
    dispatchExpect: (async () => ({ type: 'connectionProfiles', data: { profiles: PROFILES } })) as unknown as CoreClient['dispatchExpect'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function opened(overrides: Partial<Record<string, Handler>> = {}, seen: Req[] = []): Promise<ComponentFixture<Host>> {
  const f = render(overrides, seen);
  await settle(f);
  disclosure(f).click();
  await settle(f);
  return f;
}

const host = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const disclosure = (f: ComponentFixture<unknown>) => host(f).querySelector('button') as HTMLButtonElement;
const field = (f: ComponentFixture<unknown>, key: string) => host(f).querySelector(`#consolidation-${key}`) as HTMLInputElement;
const sets = (seen: Req[]) => seen.filter((r) => r.type === 'memoryConsolidationConfigSet');
const err = (f: ComponentFixture<unknown>) => host(f).querySelector('#err')?.textContent ?? '';
const select = (f: ComponentFixture<unknown>) => host(f).querySelector('#consolidation-profile') as HTMLSelectElement;

async function commit(f: ComponentFixture<unknown>, key: string, value: string): Promise<void> {
  const input = field(f, key);
  input.value = value;
  input.dispatchEvent(new Event('input'));
  await settle(f);
  input.dispatchEvent(new Event('blur'));
  await settle(f);
}

describe('MemoryConsolidationParticulars (P4.D300)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('starts collapsed behind "▸ Consolidation particulars"; opens to "▾" with aria-expanded', async () => {
    const f = render();
    await settle(f);
    expect(disclosure(f).textContent?.trim()).toBe('▸ Consolidation particulars');
    expect(disclosure(f).getAttribute('aria-expanded')).toBe('false');
    expect(host(f).querySelector('#consolidation-profile')).toBeNull();
    disclosure(f).click();
    await settle(f);
    expect(disclosure(f).textContent?.trim()).toBe('▾ Consolidation particulars');
    expect(disclosure(f).getAttribute('aria-expanded')).toBe('true');
  });

  it('seven fields in v4\'s order — Cluster threshold (Built-in) IN, retention OUT — with labels, help, bounds', async () => {
    const f = await opened();
    const rows = Array.from(host(f).querySelectorAll('.grid > div')).map((d) => [
      d.querySelector('input')!.id,
      d.querySelector('label')!.textContent,
      d.querySelector('input')!.getAttribute('min'),
      d.querySelector('input')!.getAttribute('max'),
      d.querySelector('input')!.getAttribute('step'),
    ]);
    expect(rows).toEqual([
      ['consolidation-clusterThreshold', 'Cluster threshold', '0', '1', '0.01'],
      ['consolidation-tfidfClusterThreshold', 'Cluster threshold (Built-in)', '0', '1', '0.01'],
      ['consolidation-minClusterSize', 'Smallest cluster', '2', null, '1'],
      ['consolidation-maxClusterSize', 'Largest cluster', '2', null, '1'],
      ['consolidation-matureAfterDays', 'Mature after (days)', '0', null, '1'],
      ['consolidation-maxClustersPerRun', 'Clusters per run', '1', null, '1'],
      ['consolidation-watermark', 'Watermark', '1', null, '1'],
    ]);
    expect(host(f).querySelector('#consolidation-coldRetentionDays')).toBeNull();
    expect(field(f, 'tfidfClusterThreshold').parentElement!.querySelector('p')!.textContent).toBe(
      'The same measure when the default embedding profile is the Built-in TF-IDF one, which reckons likeness far more sparingly.',
    );
    expect(field(f, 'clusterThreshold').value).toBe('0.9');
    expect(field(f, 'tfidfClusterThreshold').value).toBe('0.3');
  });

  it('a valid value saves ONE key; an unchanged one saves nothing', async () => {
    const seen: Req[] = [];
    const f = await opened({}, seen);
    await commit(f, 'tfidfClusterThreshold', '0.25');
    await commit(f, 'watermark', '150');
    expect(sets(seen)).toEqual([{ type: 'memoryConsolidationConfigSet', config: { tfidfClusterThreshold: 0.25 } }]);
    expect(field(f, 'tfidfClusterThreshold').value).toBe('0.25');
  });

  it('both error shapes; the draft clears FIRST so the stored value shows again; no save', async () => {
    const seen: Req[] = [];
    const f = await opened({}, seen);
    await commit(f, 'clusterThreshold', '1.5');
    expect(err(f)).toBe('Cluster threshold must be a number between 0 and 1');
    expect(field(f, 'clusterThreshold').value).toBe('0.9');
    await commit(f, 'minClusterSize', '2.5');
    expect(err(f)).toBe('Smallest cluster must be a whole number of at least 2');
    await commit(f, 'matureAfterDays', '-1');
    expect(err(f)).toBe('Mature after (days) must be a number of at least 0');
    await commit(f, 'watermark', '');
    expect(err(f)).toBe('Watermark must be a whole number of at least 1');
    expect(field(f, 'watermark').value).toBe('150');
    expect(sets(seen)).toEqual([]);
  });

  it('the profile select: cheap LLM → null, the options, the missing-profile option', async () => {
    const seen: Req[] = [];
    const f = await opened({ memoryConsolidationConfigGet: () => ({ success: true, settings: { ...STORED, connectionProfileId: 'gone' } }) }, seen);
    expect(Array.from(select(f).options).map((o) => [o.value, o.textContent])).toEqual([
      ['', 'The cheap LLM (default)'],
      ['gone', '(a profile that has since left the building)'],
      ['p1', 'House (OPENAI • gpt-4o)'],
      ['p2', 'Bare (OLLAMA)'],
    ]);
    expect(select(f).value).toBe('gone');
    select(f).value = '';
    select(f).dispatchEvent(new Event('change'));
    await settle(f);
    expect(sets(seen)).toEqual([{ type: 'memoryConsolidationConfigSet', config: { connectionProfileId: null } }]);
  });

  it('a FAILED profile save snaps the select back to the stored id (L-3 / R-E)', async () => {
    const f = await opened({
      memoryConsolidationConfigSet: () => {
        throw new CoreDispatchError({ kind: 'bad-request', message: 'No such profile' });
      },
    });
    expect(select(f).value).toBe('p1');
    select(f).value = 'p2';
    select(f).dispatchEvent(new Event('change'));
    await settle(f);
    expect(select(f).value).toBe('p1');
    expect(err(f)).toBe('No such profile');
  });
});
