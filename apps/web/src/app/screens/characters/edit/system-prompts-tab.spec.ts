import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { coreStreamStub } from '../../../core/core-client.testing';
import type { CharacterSystemPrompt } from '../../../core/core-contract';
import { RichEditor } from '../../../editor/rich-editor';
import { CharacterSystemPromptsTab } from './system-prompts-tab';

function prompt(over: Partial<CharacterSystemPrompt> = {}): CharacterSystemPrompt {
  return {
    id: 'p1',
    name: 'Romantic',
    content: 'Speak tenderly.',
    isDefault: false,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

function stubClient(
  prompts: CharacterSystemPrompt[],
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  return {
    // The stream surface is required since P4.D165 hosted the Subprompts
    // section here: its shared query injects `RealtimeService`, whose
    // constructor subscribes to `events$` (`core-client.testing.ts`'s header).
    ...coreStreamStub(),
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      if (req.type === 'characterPromptList') {
        return { prompts };
      }
      if (req.type === 'characterSubpromptList') {
        return { subprompts: [] };
      }
      if (req.type === 'promptTemplateList') {
        return { templates: TEMPLATES, count: TEMPLATES.length };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(
  client: Partial<CoreClient>,
): Promise<ComponentFixture<CharacterSystemPromptsTab>> {
  TestBed.configureTestingModule({
    imports: [CharacterSystemPromptsTab],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CharacterSystemPromptsTab);
  fixture.componentRef.setInput('characterId', 'char-1');
  fixture.componentRef.setInput('characterName', 'Bertie');
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** The prompt modal's content editor (qt-markdown-field's RichEditor handle). */
function contentEditor(fixture: ComponentFixture<unknown>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

function clickButtonWithText(fixture: ComponentFixture<unknown>, text: string): void {
  const button = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === text,
  ) as HTMLButtonElement;
  button.click();
  fixture.detectChanges();
}

/** The wire shape P4.83's verb answers: a built-in (no `userId` key at all —
 *  the column is NULL and v4's wire OMITS it) beside a user template. */
const TEMPLATES = [
  {
    id: 'bt-1',
    name: 'MODERN General',
    content: '# A general prompt',
    description: 'GENERAL prompt optimized for MODERN models',
    isBuiltIn: true,
    category: 'GENERAL',
    modelHint: 'MODERN',
    tags: [],
    createdAt: '2020-01-01T00:00:00.000Z',
    updatedAt: '2020-01-01T00:00:00.000Z',
  },
  {
    id: 'ut-1',
    userId: 'u1',
    name: 'My Own Prompt',
    content: 'Mine.',
    isBuiltIn: false,
    tags: [],
    createdAt: '2020-01-01T00:00:00.000Z',
    updatedAt: '2020-01-01T00:00:00.000Z',
  },
];

function clickImport(fixture: ComponentFixture<CharacterSystemPromptsTab>): void {
  const button = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === 'Import Template',
  ) as HTMLButtonElement;
  expect(button.disabled).toBe(false);
  button.click();
  fixture.detectChanges();
}

describe('CharacterSystemPromptsTab', () => {
  it('shows the empty state and opens the Import Template modal with the seeded catalogue (P4.83)', async () => {
    const fixture = await render(stubClient([]));
    expect(fixture.nativeElement.textContent).toContain('No system prompts yet');
    clickImport(fixture);
    await settle(fixture);
    // The `p4.9k`-era "always empty" divergence is CLOSED: the catalogue the
    // `promptTemplateList` verb answers is what renders.
    expect(fixture.nativeElement.textContent).not.toContain('No templates available');
    expect(fixture.nativeElement.textContent).toContain('Sample Prompts');
    expect(fixture.nativeElement.textContent).toContain('MODERN General');
    expect(fixture.nativeElement.textContent).toContain('My Own Prompt');
  });

  it('opening the Import Template modal ALWAYS refetches (v4 `openImportModal`)', async () => {
    const seen: string[] = [];
    const fixture = await render(stubClient([], (req) => seen.push(req.type)));

    clickImport(fixture);
    await settle(fixture);
    expect(seen.filter((t) => t === 'promptTemplateList')).toHaveLength(1);

    // Close and reopen: the editor host refetches every time — deliberately
    // NOT the new-character host's fetch-only-when-empty guard.
    fixture.componentInstance['importModalOpen'].set(false);
    await settle(fixture);
    clickImport(fixture);
    await settle(fixture);
    expect(seen.filter((t) => t === 'promptTemplateList')).toHaveLength(2);
  });

  it('a failed template fetch leaves the catalogue untouched and logs (v4 `fetchTemplates`)', async () => {
    const failing: Partial<CoreClient> = {
      ...coreStreamStub(),
      dispatchData: (async (req: { type: string }) => {
        if (req.type === 'characterPromptList') return { prompts: [] };
        if (req.type === 'characterSubpromptList') return { subprompts: [] };
        if (req.type === 'promptTemplateList') throw new Error('boom');
        return {};
      }) as CoreClient['dispatchData'],
    };
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    try {
      const fixture = await render(failing);
      clickImport(fixture);
      await settle(fixture);
      // v4 leaves `templates` as it was, so the modal shows its own empty copy.
      expect(fixture.nativeElement.textContent).toContain('No templates available');
      expect(fixture.nativeElement.textContent).not.toContain('Loading templates...');
      expect(errors).toHaveBeenCalledWith('Error fetching templates', { error: 'boom' });
    } finally {
      errors.mockRestore();
    }
  });

  it('creating a prompt dispatches characterPromptCreate', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([], (req) => seen.push(req)));

    clickButtonWithText(fixture, '+ Add Prompt');
    await settle(fixture);
    const nameInput = fixture.nativeElement.querySelector(
      'input[placeholder="e.g., Romantic, Companion, Professional"]',
    ) as HTMLInputElement;
    nameInput.value = 'Romantic';
    nameInput.dispatchEvent(new Event('input'));
    contentEditor(fixture).setMarkdown('Speak tenderly.');
    await settle(fixture);

    clickButtonWithText(fixture, 'Create');
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const createCall = seen.find((r) => r.type === 'characterPromptCreate');
    expect(createCall).toBeTruthy();
    expect(createCall!['characterId']).toBe('char-1');
    expect(createCall!['name']).toBe('Romantic');
    expect(createCall!['content']).toBe('Speak tenderly.');
  });

  describe('the create form’s default seed (v4 `openCreateModal`)', () => {
    /** The create modal's "Set as default prompt" checkbox. */
    function isDefaultBox(fixture: ComponentFixture<unknown>): HTMLInputElement {
      return fixture.nativeElement.querySelector('#isDefault') as HTMLInputElement;
    }

    it('a character’s FIRST prompt opens the form already starred, and the create carries it', async () => {
      const seen: Array<{ type: string; [k: string]: unknown }> = [];
      const fixture = await render(stubClient([], (req) => seen.push(req)));

      clickButtonWithText(fixture, '+ Add Prompt');
      await settle(fixture);
      expect(isDefaultBox(fixture).checked).toBe(true);

      const nameInput = fixture.nativeElement.querySelector(
        'input[placeholder="e.g., Romantic, Companion, Professional"]',
      ) as HTMLInputElement;
      nameInput.value = 'Romantic';
      nameInput.dispatchEvent(new Event('input'));
      contentEditor(fixture).setMarkdown('Speak tenderly.');
      await settle(fixture);
      clickButtonWithText(fixture, 'Create');
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();

      const createCall = seen.find((r) => r.type === 'characterPromptCreate');
      expect(createCall).toBeTruthy();
      expect(createCall!['isDefault']).toBe(true);
    });

    it('a SECOND prompt opens the form unstarred (the seed is `prompts.length === 0`, not always)', async () => {
      const fixture = await render(stubClient([prompt({ isDefault: true })]));

      clickButtonWithText(fixture, '+ Add Prompt');
      await settle(fixture);
      expect(isDefaultBox(fixture).checked).toBe(false);
    });
  });

  it('editing a prompt dispatches characterPromptUpdate', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([prompt()], (req) => seen.push(req)));

    const editButton = fixture.nativeElement.querySelector('[title="Edit"]') as HTMLButtonElement;
    editButton.click();
    fixture.detectChanges();
    await settle(fixture);

    clickButtonWithText(fixture, 'Update');
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const updateCall = seen.find((r) => r.type === 'characterPromptUpdate');
    expect(updateCall).toBeTruthy();
    expect(updateCall!['promptId']).toBe('p1');
  });

  it('the edit modal seeds the markdown field and re-saves it byte-identical', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(
      stubClient([prompt({ content: 'Speak *tenderly*, and never of the rain.' })], (req) =>
        seen.push(req),
      ),
    );

    const editButton = fixture.nativeElement.querySelector('[title="Edit"]') as HTMLButtonElement;
    editButton.click();
    fixture.detectChanges();
    await settle(fixture);

    expect(contentEditor(fixture).getMarkdown()).toBe('Speak *tenderly*, and never of the rain.');

    // Save without touching the field: the stored bytes must round-trip
    // untouched (the absorb-once seam means the mount is not an edit).
    clickButtonWithText(fixture, 'Update');
    await settle(fixture);

    const updateCall = seen.find((r) => r.type === 'characterPromptUpdate');
    expect(updateCall!['content']).toBe('Speak *tenderly*, and never of the rain.');
  });

  it('an edit made in the markdown field reaches the update payload', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([prompt()], (req) => seen.push(req)));

    const editButton = fixture.nativeElement.querySelector('[title="Edit"]') as HTMLButtonElement;
    editButton.click();
    fixture.detectChanges();
    await settle(fixture);

    contentEditor(fixture).setMarkdown('Speak **plainly**.');
    await settle(fixture);
    clickButtonWithText(fixture, 'Update');
    await settle(fixture);

    const updateCall = seen.find((r) => r.type === 'characterPromptUpdate');
    expect(updateCall!['content']).toBe('Speak **plainly**.');
  });

  it('the star button dispatches characterPromptSetDefault', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([prompt()], (req) => seen.push(req)));

    const starButton = fixture.nativeElement.querySelector(
      '[title="Set as default"]',
    ) as HTMLButtonElement;
    starButton.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const setDefaultCall = seen.find((r) => r.type === 'characterPromptSetDefault');
    expect(setDefaultCall).toBeTruthy();
    expect(setDefaultCall!['promptId']).toBe('p1');
  });

  /**
   * v4 `handleSetDefault` post-`baa85e19b` (bug 154). The badge moves in the
   * query cache BEFORE the round trip; a failure restores the snapshot and
   * THEN refetches the server's word, in that order.
   *
   * The (a) arm holds the dispatch on a deferred promise and asserts BETWEEN
   * the click and its resolution — the only place an optimistic write is
   * distinguishable from the refresh that follows it
   * (`e2e-assertion-can-read-the-pre-click-state`'s class).
   */
  describe('the star’s optimistic write (v4 baa85e19b)', () => {
    /**
     * A client whose `characterPromptSetDefault` resolves (or rejects) on
     * demand, and — with `holdRelist` — whose SECOND `characterPromptList`
     * hangs until released. Holding the relist is what makes the rollback
     * measurable: after a rejection the cache is restored synchronously and
     * the refetch is still in flight, and only in that window does a restored
     * snapshot differ from "whatever the server says". (Without it, mutation
     * M4 — the rollback deleted outright — SURVIVED: the refetch alone
     * restores the same rows.)
     */
    function heldClient(
      prompts: CharacterSystemPrompt[],
      opts: { listsAfter?: CharacterSystemPrompt[]; holdRelist?: boolean } = {},
    ): {
      client: Partial<CoreClient>;
      settle: (outcome: 'resolve' | 'reject') => void;
      releaseRelist: () => void;
      listCalls: () => number;
    } {
      let release!: (outcome: 'resolve' | 'reject') => void;
      const held = new Promise<Record<string, unknown>>((res, rej) => {
        release = (outcome) => (outcome === 'resolve' ? res({}) : rej(new Error('nope')));
      });
      let releaseRelist!: () => void;
      const relistGate = new Promise<void>((res) => {
        releaseRelist = res;
      });
      let lists = 0;
      return {
        listCalls: () => lists,
        settle: (outcome) => release(outcome),
        releaseRelist: () => releaseRelist(),
        client: {
          ...coreStreamStub(),
          dispatchData: (async (req: { type: string }) => {
            if (req.type === 'characterPromptList') {
              lists += 1;
              if (lists > 1 && opts.holdRelist) await relistGate;
              return { prompts: lists > 1 ? (opts.listsAfter ?? prompts) : prompts };
            }
            if (req.type === 'characterSubpromptList') return { subprompts: [] };
            if (req.type === 'characterPromptSetDefault') return held;
            return {};
          }) as CoreClient['dispatchData'],
        },
      };
    }

    const two = [
      prompt({ id: 'p1', name: 'Romantic', isDefault: true }),
      prompt({ id: 'p2', name: 'Brisk', isDefault: false }),
    ];

    it('(a) moves the badge BEFORE the dispatch resolves', async () => {
      const held = heldClient(two);
      const fixture = await render(held.client);

      const star = fixture.nativeElement.querySelectorAll(
        '[title="Set as default"]',
      )[0] as HTMLButtonElement;
      star.click();
      await settle(fixture);

      // The dispatch has NOT answered yet — this is the optimistic write alone.
      const flags = fixture.componentInstance['prompts']().map((p) => [p.id, p.isDefault]);
      expect(flags).toEqual([
        ['p1', false],
        ['p2', true],
      ]);
      expect(fixture.nativeElement.textContent).toContain('Default');

      held.settle('resolve');
      await settle(fixture);
    });

    it('(b) a rejection restores the snapshot BEFORE the refetch has answered', async () => {
      // The relist hangs, so the only thing that can put the badge back in
      // this window is the rollback itself.
      const held = heldClient(two, { holdRelist: true });
      const fixture = await render(held.client);
      const listsBefore = held.listCalls();

      (
        fixture.nativeElement.querySelectorAll('[title="Set as default"]')[0] as HTMLButtonElement
      ).click();
      await settle(fixture);
      expect(fixture.componentInstance['prompts']().map((p) => p.isDefault)).toEqual([false, true]);

      held.settle('reject');
      await settle(fixture);

      // The refetch is still in flight — and the badge is already back.
      expect(fixture.componentInstance['prompts']().map((p) => p.isDefault)).toEqual([true, false]);
      expect(held.listCalls()).toBeGreaterThan(listsBefore);

      held.releaseRelist();
      await settle(fixture);
      expect(fixture.componentInstance['prompts']().map((p) => p.isDefault)).toEqual([true, false]);
      expect(fixture.nativeElement.textContent).toContain('nope');
    });

    it('(c) a success refreshes, so the server’s list is what finally renders', async () => {
      // The server says p2 is default — and ALSO renames it, a byte the
      // optimistic write could not have invented.
      const held = heldClient(two, {
        listsAfter: [
          prompt({ id: 'p1', name: 'Romantic', isDefault: false }),
          prompt({ id: 'p2', name: 'Brisk (server)', isDefault: true }),
        ],
      });
      const fixture = await render(held.client);
      const listsBefore = held.listCalls();

      (
        fixture.nativeElement.querySelectorAll('[title="Set as default"]')[0] as HTMLButtonElement
      ).click();
      await settle(fixture);
      held.settle('resolve');
      await settle(fixture);
      await settle(fixture);

      expect(held.listCalls()).toBeGreaterThan(listsBefore);
      expect(fixture.nativeElement.textContent).toContain('Brisk (server)');
      expect(fixture.componentInstance['prompts']().map((p) => p.isDefault)).toEqual([false, true]);
      expect(fixture.componentInstance['error']()).toBeNull();
    });
  });

  it('the trash button dispatches characterPromptDelete', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient([prompt()], (req) => seen.push(req)));

    const deleteButton = fixture.nativeElement.querySelector(
      '[title="Delete"]',
    ) as HTMLButtonElement;
    deleteButton.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const deleteCall = seen.find((r) => r.type === 'characterPromptDelete');
    expect(deleteCall).toBeTruthy();
    expect(deleteCall!['promptId']).toBe('p1');
  });
});
