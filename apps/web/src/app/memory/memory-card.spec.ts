import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { MemoryDto } from '../core/core-contract';
import { MemoryCard } from './memory-card';

function mem(over: Partial<MemoryDto>): MemoryDto {
  return {
    id: 'm1',
    userId: 'u1',
    characterId: 'c1',
    content: 'A short memory.',
    summary: 'The tea incident',
    keywords: [],
    tags: [],
    importance: 0.5,
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
    reinforcedImportance: 0.5,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

function render(memory: MemoryDto): ComponentFixture<MemoryCard> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [MemoryCard] });
  const fixture = TestBed.createComponent(MemoryCard);
  fixture.componentRef.setInput('memory', memory);
  fixture.detectChanges();
  return fixture;
}

describe('MemoryCard', () => {
  it('renders the summary and content', () => {
    const fixture = render(mem({ summary: 'The tea incident', content: 'Jeeves prefers Earl Grey.' }));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('The tea incident');
    expect(text).toContain('Jeeves prefers Earl Grey.');
  });

  it('shows the High importance bucket for >= 0.7', () => {
    const fixture = render(mem({ importance: 0.8 }));
    const label = fixture.nativeElement.querySelector('.qt-text-label-xs') as HTMLElement;
    expect(label.textContent?.trim()).toBe('High');
    expect(label.className).toContain('qt-text-destructive');
    expect(label.getAttribute('title')).toBe('Importance: 80%');
  });

  it('shows Medium for >= 0.4 and Low below', () => {
    expect(
      (render(mem({ importance: 0.5 })).nativeElement.querySelector('.qt-text-label-xs') as HTMLElement)
        .textContent?.trim(),
    ).toBe('Medium');
    expect(
      (render(mem({ importance: 0.2 })).nativeElement.querySelector('.qt-text-label-xs') as HTMLElement)
        .textContent?.trim(),
    ).toBe('Low');
  });

  it('marks AUTO vs MANUAL source', () => {
    expect(render(mem({ source: 'AUTO' })).nativeElement.textContent).toContain('Auto');
    expect(render(mem({ source: 'MANUAL' })).nativeElement.textContent).toContain('Manual');
  });

  it('shows the expand toggle only when content exceeds 150 chars', () => {
    const short = render(mem({ content: 'x'.repeat(150) }));
    expect(short.nativeElement.textContent).not.toContain('Show more');
    const long = render(mem({ content: 'x'.repeat(151) }));
    expect(long.nativeElement.textContent).toContain('Show more');
    const btn = long.nativeElement.querySelector('button') as HTMLButtonElement;
    btn.click();
    long.detectChanges();
    expect(long.nativeElement.textContent).toContain('Show less');
  });

  it('renders keyword chips and read-only tag badges', () => {
    const fixture = render(
      mem({ keywords: ['tea', 'earl-grey'], tagDetails: [{ id: 't1', name: 'preferences' }] }),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('tea');
    expect(text).toContain('earl-grey');
    expect(text).toContain('preferences');
  });

  it('shows the Source link only for AUTO memories with chatId + sourceMessageId', () => {
    // AUTO but missing provenance → no link.
    expect(render(mem({ source: 'AUTO' })).nativeElement.textContent).not.toContain('Source');
    // MANUAL with provenance → no link.
    expect(
      render(mem({ source: 'MANUAL', chatId: 'chat-1', sourceMessageId: 'msg-1' })).nativeElement
        .textContent,
    ).not.toContain('Source');
    // AUTO with both → link.
    const fixture = render(mem({ source: 'AUTO', chatId: 'chat-1', sourceMessageId: 'msg-1' }));
    expect(fixture.nativeElement.textContent).toContain('Source');
  });

  it('emits navigateToSource with chatId + messageId', () => {
    const fixture = render(mem({ source: 'AUTO', chatId: 'chat-9', sourceMessageId: 'msg-9' }));
    let seen: { chatId: string; messageId: string } | null = null;
    fixture.componentInstance.navigateToSource.subscribe((e) => (seen = e));
    const sourceBtn = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((b) => b.textContent?.includes('Source'))!;
    sourceBtn.click();
    expect(seen).toEqual({ chatId: 'chat-9', messageId: 'msg-9' });
  });

  it('emits edit and delete', () => {
    const memory = mem({ id: 'mx' });
    const fixture = render(memory);
    let edited: MemoryDto | null = null;
    let deletedId: string | null = null;
    fixture.componentInstance.edit.subscribe((m) => (edited = m));
    fixture.componentInstance.delete.subscribe((id) => (deletedId = id));
    const buttons = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    );
    buttons.find((b) => b.textContent?.trim() === 'Edit')!.click();
    buttons.find((b) => b.textContent?.trim() === 'Delete')!.click();
    expect(edited).toBe(memory);
    expect(deletedId).toBe('mx');
  });

  it('shows Deleting... and disables delete while deleting', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [MemoryCard] });
    const fixture = TestBed.createComponent(MemoryCard);
    fixture.componentRef.setInput('memory', mem({}));
    fixture.componentRef.setInput('isDeleting', true);
    fixture.detectChanges();
    const del = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((b) => b.textContent?.includes('Deleting'))!;
    expect(del).toBeTruthy();
    expect(del.disabled).toBe(true);
  });
});
