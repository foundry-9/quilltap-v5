import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { MemoryDto } from '../core/core-contract';
import { RichEditor } from '../editor/rich-editor';
import { ToastService } from '../ui/toast.service';
import { MemoryEditor } from './memory-editor';

function mem(over: Partial<MemoryDto>): MemoryDto {
  return {
    id: 'm1',
    userId: 'u1',
    characterId: 'c1',
    content: 'Old content',
    summary: 'Old summary',
    keywords: ['a', 'b'],
    tags: [],
    importance: 0.6,
    source: 'MANUAL',
    chatId: null,
    projectId: null,
    aboutCharacterId: null,
    sourceMessageId: null,
    witnessedContext: null,
    lastAccessedAt: null,
    reinforcementCount: 0,
    lastReinforcedAt: null,
    relatedMemoryIds: [],
    reinforcedImportance: 0.6,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

function render(
  memory: MemoryDto | null,
  dispatch: (req: DispatchReq) => Promise<Record<string, unknown>>,
  toasts?: { showSuccess: (m: string) => void; showError: (m: string) => void },
): ComponentFixture<MemoryEditor> {
  const client: Partial<CoreClient> = {
    dispatchData: dispatch as CoreClient['dispatchData'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryEditor],
    providers: [
      { provide: CoreClient, useValue: client },
      ...(toasts ? [{ provide: ToastService, useValue: toasts }] : []),
    ],
  });
  const fixture = TestBed.createComponent(MemoryEditor);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.componentRef.setInput('memory', memory);
  fixture.detectChanges();
  return fixture;
}

function fieldByLabel(fixture: ComponentFixture<MemoryEditor>, id: string): HTMLInputElement {
  return fixture.nativeElement.querySelector(`#${id}`) as HTMLInputElement;
}

/** The content markdown editor (qt-markdown-field's RichEditor handle). */
function contentEditor(fixture: ComponentFixture<MemoryEditor>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('MemoryEditor', () => {
  it('gives the content editor v4’s minHeight', () => {
    // v4 memory-editor.tsx:154 — `minHeight="10rem"`. The mechanism is proven in
    // markdown-field.spec; this pins the VALUE, which no other spec would catch.
    const fixture = render(mem({}), async () => ({}));
    const editor = fixture.nativeElement.querySelector('qt-rich-editor') as HTMLElement;
    expect(editor.style.minHeight).toBe('10rem');
  });

  it('the importance slider wears qt-range, not the old appearance-none idiom (v4 5f56f7a7d)', () => {
    const fixture = render(mem({}), async () => ({}));
    expect(fieldByLabel(fixture, 'importance').className).toBe('qt-range w-full');
  });

  it('titles Add Memory on create and Edit Memory on edit', () => {
    expect(render(null, async () => ({})).nativeElement.textContent).toContain('Add Memory');
    expect(render(mem({}), async () => ({})).nativeElement.textContent).toContain('Edit Memory');
  });

  it('seeds the form from the memory (keywords comma-joined, importance label)', async () => {
    const fixture = render(mem({ summary: 'S', content: 'C', keywords: ['x', 'y'], importance: 0.8 }), async () => ({}));
    await settle(fixture);
    expect(fieldByLabel(fixture, 'summary').value).toBe('S');
    expect(contentEditor(fixture).getMarkdown()).toBe('C');
    expect(fieldByLabel(fixture, 'keywords').value).toBe('x, y');
    expect(fixture.nativeElement.textContent).toContain('Importance: High (80%)');
  });

  it('POSTs a create with split keywords and NO source (v4 c2795733f)', async () => {
    const dispatch = vi.fn(async () => ({ memory: mem({ id: 'new' }) }));
    const fixture = render(null, dispatch);
    fieldByLabel(fixture, 'summary').value = 'New summary';
    fieldByLabel(fixture, 'summary').dispatchEvent(new Event('input'));
    contentEditor(fixture).setMarkdown('New content');
    fieldByLabel(fixture, 'keywords').value = 'one, two ,three';
    fieldByLabel(fixture, 'keywords').dispatchEvent(new Event('input'));
    await settle(fixture);

    let saved = false;
    fixture.componentInstance.saved.subscribe(() => (saved = true));
    fixture.componentInstance['submit']();
    await settle(fixture);

    expect(dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'memoryCreate',
        memory: expect.objectContaining({
          characterId: 'c1',
          summary: 'New summary',
          content: 'New content',
          keywords: ['one', 'two', 'three'],
        }),
      }),
    );
    const req = (dispatch.mock.calls[0] as unknown as [DispatchReq & { memory: Record<string, unknown> }])[0];
    // v4's payload key order (`memory-editor.tsx:60-66`), `source` gone.
    expect(Object.keys(req.memory)).toEqual(['characterId', 'content', 'summary', 'keywords', 'importance']);
    expect(saved).toBe(true);
  });

  it('PUTs an update for an existing memory', async () => {
    const dispatch = vi.fn(async () => ({ memory: mem({}) }));
    const fixture = render(mem({ id: 'm7' }), dispatch);
    fixture.componentInstance['submit']();
    await settle(fixture);
    expect(dispatch).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'memoryUpdate', memoryId: 'm7' }),
    );
  });

  it('surfaces a save error and does not emit saved', async () => {
    const dispatch = vi.fn(async () => {
      throw new Error('Embedding provider unavailable');
    });
    const fixture = render(mem({}), dispatch);
    let saved = false;
    fixture.componentInstance.saved.subscribe(() => (saved = true));
    fixture.componentInstance['submit']();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('Embedding provider unavailable');
    expect(saved).toBe(false);
  });

  it('does not submit when summary or content is blank', async () => {
    const dispatch = vi.fn(async () => ({}));
    const fixture = render(null, dispatch); // both blank on create
    fixture.componentInstance['submit']();
    await settle(fixture);
    expect(dispatch).not.toHaveBeenCalled();
  });
});

/**
 * P4.D276 Tier 2 item 16's open question CLOSES at the pin: v4 `c2795733f`
 * dropped `source: 'MANUAL'` from the editor's payload (create AND edit), so
 * editing a digest sends exactly v4's PUT body — nothing that could re-source
 * it (P4.D300).
 */
describe('MemoryEditor — editing a digest (P4.D276 → P4.D300)', () => {
  it('sends the bag v4 sends — no source — for a CONSOLIDATED memory', async () => {
    const dispatch = vi.fn(async (_req: DispatchReq) => ({ memory: mem({}) }));
    const fixture = render(
      mem({ id: 'dig-1', source: 'CONSOLIDATED', tier: 'hot', consolidatedFrom: ['a', 'b'] }),
      dispatch,
    );
    fixture.componentInstance['submit']();
    await settle(fixture);
    expect(dispatch).toHaveBeenCalledTimes(1);
    const req = dispatch.mock.calls[0][0] as DispatchReq & { memory: Record<string, unknown> };
    expect(req.type).toBe('memoryUpdate');
    expect(req['memoryId']).toBe('dig-1');
    expect(Object.keys(req.memory)).toEqual(['characterId', 'content', 'summary', 'keywords', 'importance']);
  });
});

/**
 * P4.D300 L-5 — the create reply's `action` (C2: `MemoryCreate` answers
 * `{ memory, action }`) stands in for v4's 200-vs-201 split
 * (`memory-editor.tsx:86-91` at the pin: `merged = !isEditing && status === 200`;
 * v4 answers 201 iff `admissionInserted` — `insert` / `insert-linked`). An
 * ABSENT `action` (a pre-round server, which only ever inserted) reads as an
 * insert.
 */
describe('MemoryEditor — the create toast reads admission\'s action (P4.D300 L-5)', () => {
  const REINFORCED = 'Already on file — the existing memory has been reinforced instead';

  async function createWith(reply: Record<string, unknown>): Promise<string[]> {
    const shown: string[] = [];
    const toasts = { showSuccess: (m: string) => shown.push(m), showError: () => undefined };
    const fixture = render(null, vi.fn(async () => reply), toasts);
    fieldByLabel(fixture, 'summary').value = 'S';
    fieldByLabel(fixture, 'summary').dispatchEvent(new Event('input'));
    contentEditor(fixture).setMarkdown('C');
    await settle(fixture);
    fixture.componentInstance['submit']();
    await settle(fixture);
    return shown;
  }

  it.each([
    ['insert', 'Memory created'],
    ['insert-linked', 'Memory created'],
    ['reinforce', REINFORCED],
    ['absorb', REINFORCED],
    ['promote-and-reinforce', REINFORCED],
  ])('action %s → %s', async (action, toast) => {
    expect(await createWith({ memory: mem({ id: 'n' }), action })).toEqual([toast]);
  });

  it('an absent action reads as an insert → Memory created', async () => {
    expect(await createWith({ memory: mem({ id: 'n' }) })).toEqual(['Memory created']);
  });

  it('an edit toasts Memory updated whatever the reply carries', async () => {
    const shown: string[] = [];
    const toasts = { showSuccess: (m: string) => shown.push(m), showError: () => undefined };
    const fixture = render(mem({ id: 'm9' }), vi.fn(async () => ({ memory: mem({}), action: 'reinforce' })), toasts);
    fixture.componentInstance['submit']();
    await settle(fixture);
    expect(shown).toEqual(['Memory updated']);
  });
});
