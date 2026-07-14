import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { MemoryDto } from '../core/core-contract';
import { RichEditor } from '../editor/rich-editor';
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
): ComponentFixture<MemoryEditor> {
  const client: Partial<CoreClient> = {
    dispatchData: dispatch as CoreClient['dispatchData'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryEditor],
    providers: [{ provide: CoreClient, useValue: client }],
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

  it('POSTs a create with source MANUAL and split keywords', async () => {
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
          source: 'MANUAL',
        }),
      }),
    );
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
