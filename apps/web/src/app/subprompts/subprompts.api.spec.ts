import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import { characterKeys } from '../screens/characters/characters.api';
import {
  createSubprompt,
  deleteSubprompt,
  getSubprompt,
  injectCharacterSubprompts,
  listSubprompts,
  subpromptErrorMessage,
  updateSubprompt,
  type CharacterSubprompts,
} from './subprompts.api';

/**
 * The subprompts wire (§C.2) and the shared query helper — v4
 * `components/subprompts/useCharacterSubprompts.ts` at `2f4254b42`.
 *
 * @module subprompts/subprompts.api.spec
 */

type Req = { type: string; [k: string]: unknown };

function stub(answers: Record<string, unknown> = {}, seen: Req[] = []): CoreClient {
  return {
    ...coreStreamStub(),
    dispatchData: vi.fn(async (req: Req) => {
      seen.push(req);
      return (answers[req.type] as Record<string, unknown>) ?? {};
    }),
  } as unknown as CoreClient;
}

describe('subpromptErrorMessage — v4 `useCharacterSubprompts.ts:38-44`', () => {
  it('an Error yields its own message', () => {
    expect(subpromptErrorMessage(new Error('Subprompt not found'), 'fallback')).toBe(
      'Subprompt not found',
    );
  });

  it('a BLANK-message Error yields the caller’s sentence — not an empty toast', () => {
    // The measured difference from the shared `coreErrorMessage`, which ports
    // v4's OTHER helper (`apiErrorMessage`, `lib/query/fetcher.ts:64`) and has
    // no truthiness check. v4's subprompts helper does, and this is the arm
    // where the two disagree.
    expect(subpromptErrorMessage(new Error(''), 'Failed to create subprompt')).toBe(
      'Failed to create subprompt',
    );
  });

  it('a non-Error yields the caller’s sentence', () => {
    expect(subpromptErrorMessage('nope', 'Failed to delete subprompt')).toBe(
      'Failed to delete subprompt',
    );
    expect(subpromptErrorMessage(undefined, 'Failed to list subprompts')).toBe(
      'Failed to list subprompts',
    );
  });
});

describe('the five subprompt verbs (§C.2)', () => {
  it('characterSubpromptList sends only the character id and unwraps `subprompts`', async () => {
    const seen: Req[] = [];
    const core = stub({ characterSubpromptList: { subprompts: [{ id: 'terse' }] } }, seen);
    expect(await listSubprompts(core, 'char-1')).toEqual([{ id: 'terse' }]);
    expect(seen[0]).toEqual({ type: 'characterSubpromptList', characterId: 'char-1' });
  });

  it('an answer with no `subprompts` key reads as an empty folder', async () => {
    expect(await listSubprompts(stub(), 'char-1')).toEqual([]);
  });

  it('characterSubpromptGet carries the id as a field — v4 URL-encodes it, v5 builds no URL', async () => {
    const seen: Req[] = [];
    const core = stub({ characterSubpromptGet: { subprompt: { id: 'a b/c' } } }, seen);
    await getSubprompt(core, 'char-1', 'a b/c');
    expect(seen[0]).toEqual({
      type: 'characterSubpromptGet',
      characterId: 'char-1',
      subpromptId: 'a b/c',
    });
  });

  it('characterSubpromptCreate sends title + content and unwraps `subprompt`', async () => {
    const seen: Req[] = [];
    const core = stub({ characterSubpromptCreate: { subprompt: { id: 'be-terse' } } }, seen);
    expect(await createSubprompt(core, 'char-1', { title: 'Be terse', content: 'Body.' })).toEqual({
      id: 'be-terse',
    });
    expect(seen[0]).toEqual({
      type: 'characterSubpromptCreate',
      characterId: 'char-1',
      title: 'Be terse',
      content: 'Body.',
    });
  });

  it('characterSubpromptUpdate omits the fields the caller left out (v4: both optional)', async () => {
    const seen: Req[] = [];
    const core = stub({ characterSubpromptUpdate: { subprompt: { id: 'x' } } }, seen);
    await updateSubprompt(core, 'char-1', 'x', { title: 'New' });
    expect(seen[0]).toEqual({
      type: 'characterSubpromptUpdate',
      characterId: 'char-1',
      subpromptId: 'x',
      title: 'New',
    });
    await updateSubprompt(core, 'char-1', 'x', { content: '' });
    // An EMPTY content is a value, not an absence — the server refuses it, and
    // the client must let it get there to be refused.
    expect(seen[1]).toEqual({
      type: 'characterSubpromptUpdate',
      characterId: 'char-1',
      subpromptId: 'x',
      content: '',
    });
  });

  it('characterSubpromptDelete sends the pair and returns nothing', async () => {
    const seen: Req[] = [];
    await deleteSubprompt(stub({}, seen), 'char-1', 'x');
    expect(seen[0]).toEqual({
      type: 'characterSubpromptDelete',
      characterId: 'char-1',
      subpromptId: 'x',
    });
  });
});

@Component({
  selector: 'qt-subprompts-api-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: '',
})
class Host {
  readonly characterId = signal<string | null>('char-1');
  readonly enabled = signal(true);
  readonly api: CharacterSubprompts = injectCharacterSubprompts(() => this.characterId(), {
    enabled: () => this.enabled(),
  });
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function host(core: CoreClient): Promise<ComponentFixture<Host>> {
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

describe('injectCharacterSubprompts', () => {
  it('reads the list under `characterKeys.subprompts` and exposes it', async () => {
    const core = stub({
      characterSubpromptList: { subprompts: [{ id: 'terse', title: 'Terse' }] },
    });
    const fixture = await host(core);
    expect(fixture.componentInstance.api.subprompts()).toEqual([{ id: 'terse', title: 'Terse' }]);
    expect(characterKeys.subprompts('char-1')).toEqual(['characters', 'subprompts', 'char-1']);
  });

  it('`enabled: false` defers the read entirely, and never reports loading', async () => {
    const seen: Req[] = [];
    const core = stub({ characterSubpromptList: { subprompts: [] } }, seen);
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.enabled.set(false);
    fixture.detectChanges();
    await settle(fixture);
    expect(seen.filter((r) => r.type === 'characterSubpromptList')).toEqual([]);
    expect(fixture.componentInstance.api.isLoading()).toBe(false);

    // …and flipping it on issues the read (v4's collapsed picker, opened).
    fixture.componentInstance.enabled.set(true);
    fixture.detectChanges();
    await settle(fixture);
    expect(seen.filter((r) => r.type === 'characterSubpromptList').length).toBe(1);
  });

  it('a null character id defers the read whatever `enabled` says', async () => {
    const seen: Req[] = [];
    const core = stub({ characterSubpromptList: { subprompts: [] } }, seen);
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.characterId.set(null);
    fixture.detectChanges();
    await settle(fixture);
    expect(seen.filter((r) => r.type === 'characterSubpromptList')).toEqual([]);
  });

  it('each write invalidates the list, so the folder re-reads (v4 `onSuccess: invalidate`)', async () => {
    const seen: Req[] = [];
    const core = stub(
      {
        characterSubpromptList: { subprompts: [] },
        characterSubpromptCreate: { subprompt: { id: 'be-terse' } },
        characterSubpromptUpdate: { subprompt: { id: 'be-terse' } },
      },
      seen,
    );
    const fixture = await host(core);
    const lists = () => seen.filter((r) => r.type === 'characterSubpromptList').length;
    expect(lists()).toBe(1);

    await fixture.componentInstance.api.create({ title: 'Be terse', content: 'x' });
    await settle(fixture);
    expect(lists()).toBe(2);

    await fixture.componentInstance.api.update('be-terse', { title: 'Terser' });
    await settle(fixture);
    expect(lists()).toBe(3);

    await fixture.componentInstance.api.remove('be-terse');
    await settle(fixture);
    expect(lists()).toBe(4);
  });

  it('savePending / removePending gate the editor and the delete confirm', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const core = {
      ...coreStreamStub(),
      dispatchData: vi.fn(async (req: Req) => {
        if (req.type === 'characterSubpromptCreate') {
          await gate;
          return { subprompt: { id: 'be-terse' } };
        }
        return { subprompts: [] };
      }),
    } as unknown as CoreClient;
    const fixture = await host(core);
    expect(fixture.componentInstance.api.savePending()).toBe(false);
    const pending = fixture.componentInstance.api.create({ title: 'Be terse', content: 'x' });
    expect(fixture.componentInstance.api.savePending()).toBe(true);
    release();
    await pending;
    expect(fixture.componentInstance.api.savePending()).toBe(false);
  });

  it('a failed write clears the pending flag and does NOT invalidate', async () => {
    const seen: Req[] = [];
    const core = {
      ...coreStreamStub(),
      dispatchData: vi.fn(async (req: Req) => {
        seen.push(req);
        if (req.type === 'characterSubpromptCreate') throw new Error('A subprompt needs a title');
        return { subprompts: [] };
      }),
    } as unknown as CoreClient;
    const fixture = await host(core);
    await expect(fixture.componentInstance.api.create({ title: '', content: 'x' })).rejects.toThrow(
      'A subprompt needs a title',
    );
    await settle(fixture);
    expect(fixture.componentInstance.api.savePending()).toBe(false);
    expect(seen.filter((r) => r.type === 'characterSubpromptList').length).toBe(1);
  });
});
