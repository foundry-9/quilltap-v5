import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError, type ScenarioRef } from '../core/core-contract';
import {
  fetchScenarioSource,
  fetchScenariosEverywhere,
  injectScenarioRefBody,
  readScenarioByRef,
  scenarioKeys,
} from './scenario.api';

/**
 * The scenario data layer's `545e299bb` additions (v4 `lib/query/keys.ts:101-106`,
 * `ScenarioBrowserDialog.tsx:126-140`) — P4.D302 L-D / L-G.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

function recordingCore(answer: (req: Req) => unknown): { core: CoreClient; sent: Req[] } {
  const sent: Req[] = [];
  const core = {
    dispatchData: async (req: Req) => {
      sent.push(req);
      const out = answer(req);
      if (out instanceof Error) throw out;
      return out;
    },
  } as unknown as CoreClient;
  return { core, sent };
}

const startsWith = (key: readonly unknown[], prefix: readonly unknown[]): boolean =>
  prefix.every((part, i) => key[i] === part);

describe('scenarioKeys — the browser keys (v4 545e299bb)', () => {
  it("are v4's bytes", () => {
    expect(scenarioKeys.everywhere()).toEqual(['scenarios', 'everywhere', { includeArchived: false }]);
    expect(scenarioKeys.everywhere(true)).toEqual(['scenarios', 'everywhere', { includeArchived: true }]);
    expect(scenarioKeys.ref('mp-9', 'Scenarios/the-attic.md')).toEqual([
      'scenarios',
      'ref',
      'mp-9',
      'Scenarios/the-attic.md',
    ]);
  });

  it("sit under `scenarioKeys.all`, so every `['scenarios']` sweep reaches them", () => {
    // Tier 2 item 13, measured: v5's scenario MANAGERS (`screens/scenarios/**`)
    // hold their lists outside TanStack and invalidate nothing; the one
    // `scenarioKeys.all` invalidator is the Host's save dialog
    // (`scenario-builder/save-scenario-dialog.ts:400`). Its prefix reaches both
    // new keys — and the browser refetches on every open anyway (`staleTime: 0`).
    expect(startsWith(scenarioKeys.everywhere(), scenarioKeys.all)).toBe(true);
    expect(startsWith(scenarioKeys.everywhere(true), scenarioKeys.all)).toBe(true);
    expect(startsWith(scenarioKeys.ref('mp', 'p'), scenarioKeys.all)).toBe(true);
  });

  it('keeps the plain and archived catalogues as SEPARATE cache entries', () => {
    expect(scenarioKeys.everywhere(false)).not.toEqual(scenarioKeys.everywhere(true));
  });

  it("is invalidated by the save dialog's sweep (a real QueryClient)", async () => {
    const client = new QueryClient();
    client.setQueryData(scenarioKeys.everywhere(), { sources: [] });
    client.setQueryData(scenarioKeys.ref('mp', 'p'), { scenario: { body: 'x' } });
    await client.invalidateQueries({ queryKey: scenarioKeys.all });
    expect(client.getQueryState(scenarioKeys.everywhere())?.isInvalidated).toBe(true);
    expect(client.getQueryState(scenarioKeys.ref('mp', 'p'))?.isInvalidated).toBe(true);
  });
});

describe('the scenario fetchers (C3 §2)', () => {
  it('fetchScenariosEverywhere dispatches `scenarioEverywhere` with the archived flag', async () => {
    const { core, sent } = recordingCore(() => ({ sources: [] }));
    expect(await fetchScenariosEverywhere(core)).toEqual({ sources: [] });
    await fetchScenariosEverywhere(core, true);
    expect(sent).toEqual([
      { type: 'scenarioEverywhere', includeArchived: false },
      { type: 'scenarioEverywhere', includeArchived: true },
    ]);
  });

  it('readScenarioByRef sends the path EXACTLY as given — never encoded (R-Z)', async () => {
    const { core, sent } = recordingCore(() => ({ scenario: { body: 'b' }, owner: {} }));
    await readScenarioByRef(core, { mountPointId: 'mp 9', path: 'Scenarios/Café & Co %2F.md' });
    expect(sent).toEqual([
      { type: 'scenarioReadByRef', mountPointId: 'mp 9', path: 'Scenarios/Café & Co %2F.md' },
    ]);
  });

  it('fetchScenarioSource dispatches `chatGetScenarioSource` for the chat', async () => {
    const info = {
      source: null,
      owner: null,
      scenarioName: null,
      scenarioId: null,
      exists: false,
      changed: false,
      notes: null,
    };
    const { core, sent } = recordingCore(() => info);
    expect(await fetchScenarioSource(core, 'chat-1')).toEqual(info);
    expect(sent).toEqual([{ type: 'chatGetScenarioSource', chatId: 'chat-1' }]);
  });
});

describe('injectScenarioRefBody (v4 useScenarioRefBody)', () => {
  @Component({ template: '' })
  class Probe {
    readonly ref = signal<ScenarioRef | null>(null);
    readonly enabled = signal(true);
    readonly query = injectScenarioRefBody(
      () => this.ref(),
      () => this.enabled(),
    );
  }

  async function mount(answer: (req: Req) => unknown) {
    const { core, sent } = recordingCore(answer);
    const client = new QueryClient();
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [Probe],
      providers: [{ provide: CoreClient, useValue: core }, provideTanStackQuery(client)],
    });
    const fixture = TestBed.createComponent(Probe);
    fixture.detectChanges();
    return { fixture, sent, client };
  }

  async function settle(): Promise<void> {
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      TestBed.tick();
    }
  }

  it('reads nothing for a null ref', async () => {
    const { sent } = await mount(() => ({ scenario: { body: 'x' } }));
    await settle();
    expect(sent).toEqual([]);
  });

  it('reads the ref once set, under `scenarioKeys.ref`', async () => {
    const { fixture, sent, client } = await mount(() => ({ scenario: { body: 'A borrowed scene.' } }));
    fixture.componentInstance.ref.set({ mountPointId: 'mp-9', path: 'Scenarios/borrowed.md' });
    await settle();
    expect(sent).toEqual([
      { type: 'scenarioReadByRef', mountPointId: 'mp-9', path: 'Scenarios/borrowed.md' },
    ]);
    expect(client.getQueryData(scenarioKeys.ref('mp-9', 'Scenarios/borrowed.md'))).toEqual({
      scenario: { body: 'A borrowed scene.' },
    });
  });

  it('reads nothing while disabled', async () => {
    const { fixture, sent } = await mount(() => ({ scenario: { body: 'x' } }));
    fixture.componentInstance.enabled.set(false);
    fixture.componentInstance.ref.set({ mountPointId: 'mp-9', path: 'Scenarios/x.md' });
    await settle();
    expect(sent).toEqual([]);
  });

  it('does not retry a refused ref (v4 `retry: false`, against the app default of 1)', async () => {
    const { fixture, sent } = await mount(
      () => new CoreDispatchError({ kind: 'notFound', message: 'Scenario not found' } as never),
    );
    fixture.componentInstance.ref.set({ mountPointId: 'mp-9', path: 'Scenarios/gone.md' });
    await settle();
    await new Promise((r) => setTimeout(r, 1200));
    await settle();
    expect(sent).toHaveLength(1);
    expect(fixture.componentInstance.query.isError()).toBe(true);
  });
});
