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

  it('shows Medium for >= 0.3 and Low below (v4 importanceBand, e0ac7493a)', () => {
    const band = (importance: number) =>
      (render(mem({ importance })).nativeElement.querySelector('.qt-text-label-xs') as HTMLElement)
        .textContent?.trim();
    expect(band(0.69)).toBe('Medium');
    expect(band(0.3)).toBe('Medium');
    expect(band(0.29)).toBe('Low');
    expect(band(0.2)).toBe('Low');
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

/**
 * P4.D276 — v4 `d58548051`'s card (`components/memory/memory-card.tsx` at
 * `01a83539d`): the `Archived` chip, the `Digest` source chip, the provenance
 * line, and the handler-presence gates translated to `showActions` /
 * `canViewDigest` (R-D).
 */
describe('MemoryCard — tiers and digests (P4.D276, v4 d58548051)', () => {
  function renderWith(memory: MemoryDto, inputs: Record<string, unknown> = {}): ComponentFixture<MemoryCard> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [MemoryCard] });
    const fixture = TestBed.createComponent(MemoryCard);
    fixture.componentRef.setInput('memory', memory);
    for (const [k, v] of Object.entries(inputs)) {
      fixture.componentRef.setInput(k, v);
    }
    fixture.detectChanges();
    return fixture;
  }

  function chips(fixture: ComponentFixture<MemoryCard>): HTMLElement[] {
    return Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('span.rounded-full'),
    ) as HTMLElement[];
  }

  function provenance(fixture: ComponentFixture<MemoryCard>): HTMLElement | null {
    return (fixture.nativeElement as HTMLElement).querySelector('p.qt-text-xs.qt-text-muted.mb-2');
  }

  function buttonTexts(fixture: ComponentFixture<MemoryCard>): string[] {
    return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).map(
      (b) => b.textContent?.trim() ?? '',
    );
  }

  it('marks a CONSOLIDATED row `Digest` on the warning chip (v4 :87-94)', () => {
    const chip = chips(renderWith(mem({ source: 'CONSOLIDATED' }))).at(-1)!;
    expect(chip.textContent?.trim()).toBe('Digest');
    expect(chip.classList.contains('qt-bg-warning/10')).toBe(true);
    expect(chip.classList.contains('qt-text-warning')).toBe(true);
    expect(chip.classList.contains('qt-bg-success/10')).toBe(false);
  });

  it('keeps the AUTO / MANUAL chips on their classes (v4 :88, :92)', () => {
    const auto = chips(renderWith(mem({ source: 'AUTO' }))).at(-1)!;
    expect(auto.textContent?.trim()).toBe('Auto');
    expect(auto.classList.contains('qt-text-info')).toBe(true);
    const manual = chips(renderWith(mem({ source: 'MANUAL' }))).at(-1)!;
    expect(manual.textContent?.trim()).toBe('Manual');
    expect(manual.classList.contains('qt-text-success')).toBe(true);
  });

  it('shows the Archived chip only on a cold row, BEFORE the source chip, with its title (v4 :79-86)', () => {
    const cold = chips(renderWith(mem({ tier: 'cold', source: 'AUTO' })));
    expect(cold.map((c) => c.textContent?.trim())).toEqual(['Archived', 'Auto']);
    expect(cold[0].getAttribute('title')).toBe('Archived: out of everyday recall, still on the shelf');
    for (const cls of ['text-xs', 'px-2', 'py-0.5', 'rounded-full', 'qt-bg-muted', 'qt-text-secondary']) {
      expect(cold[0].classList.contains(cls)).toBe(true);
    }
    const hot = chips(renderWith(mem({ tier: 'hot', source: 'AUTO' })));
    expect(hot.map((c) => c.textContent?.trim())).toEqual(['Auto']);
  });

  it('dims only a cold row with opacity-75 (v4 :67)', () => {
    const root = (f: ComponentFixture<MemoryCard>) =>
      (f.nativeElement as HTMLElement).querySelector('.qt-card') as HTMLElement;
    expect(root(renderWith(mem({ tier: 'cold' }))).classList.contains('opacity-75')).toBe(true);
    expect(root(renderWith(mem({ tier: 'hot' }))).classList.contains('opacity-75')).toBe(false);
    expect(root(renderWith(mem({}))).classList.contains('opacity-75')).toBe(false);
  });

  it('writes the provenance lead byte-exact, trailing space included (v4 :101)', () => {
    expect(
      provenance(renderWith(mem({ source: 'CONSOLIDATED', consolidatedFrom: ['a'] })))!.textContent,
    ).toBe('Digested from 1 note. ');
    expect(
      provenance(renderWith(mem({ source: 'CONSOLIDATED', consolidatedFrom: ['a', 'b', 'c'] })))!
        .textContent,
    ).toBe('Digested from 3 notes. ');
  });

  it('shows no provenance for a hot digest with empty or absent members (v4 :64, :99)', () => {
    expect(provenance(renderWith(mem({ source: 'CONSOLIDATED', consolidatedFrom: [] })))).toBeNull();
    expect(provenance(renderWith(mem({ source: 'CONSOLIDATED' })))).toBeNull();
    // A non-digest row's members never count.
    expect(provenance(renderWith(mem({ source: 'AUTO', consolidatedFrom: ['a'] })))).toBeNull();
  });

  it('offers the superseded BUTTON with canViewDigest and emits the digest id (v4 :103-111)', () => {
    const fixture = renderWith(mem({ tier: 'cold', supersededById: 'digest-7' }), { canViewDigest: true });
    let seen: string | null = null;
    // Read through a structural view so this arm COMPILES against a card
    // without the output (red-first measures it behaviourally, not by TS).
    const outputs = fixture.componentInstance as unknown as Record<
      string,
      { subscribe(fn: (id: string) => void): unknown } | undefined
    >;
    outputs['viewDigest']?.subscribe((id) => (seen = id));
    const p = provenance(fixture)!;
    expect(p.textContent).toBe('Superseded by a digest');
    const btn = p.querySelector('button') as HTMLButtonElement;
    expect(btn.getAttribute('type')).toBe('button');
    expect(btn.classList.contains('text-primary')).toBe(true);
    expect(btn.classList.contains('hover:underline')).toBe(true);
    btn.click();
    expect(seen).toBe('digest-7');
  });

  it('writes the superseded TEXT without canViewDigest (v4 :113)', () => {
    const p = provenance(renderWith(mem({ tier: 'cold', supersededById: 'digest-7' })))!;
    expect(p.textContent).toBe('Superseded by a digest.');
    expect(p.querySelector('button')).toBeNull();
  });

  it('renders both halves on an archived digest with members (v4 :99-116)', () => {
    const p = provenance(
      renderWith(
        mem({ source: 'CONSOLIDATED', consolidatedFrom: ['a', 'b'], tier: 'cold', supersededById: 'd2' }),
      ),
    )!;
    expect(p.textContent).toBe('Digested from 2 notes. Superseded by a digest.');
  });

  it('shows the chip but no provenance on an archived row without supersededById', () => {
    const fixture = renderWith(mem({ tier: 'cold', supersededById: null }));
    expect(chips(fixture)[0].textContent?.trim()).toBe('Archived');
    expect(provenance(fixture)).toBeNull();
  });

  it('a hot row carrying supersededById shows no superseded line (v4 `archived &&`)', () => {
    expect(provenance(renderWith(mem({ tier: 'hot', supersededById: 'd9' }), { canViewDigest: true }))).toBeNull();
  });

  it('[showActions]=false renders no Edit, Delete or Source, even on a sourced AUTO row (R-D)', () => {
    const fixture = renderWith(
      mem({ source: 'AUTO', chatId: 'chat-1', sourceMessageId: 'msg-1' }),
      { showActions: false },
    );
    const texts = buttonTexts(fixture);
    expect(texts).not.toContain('Edit');
    expect(texts).not.toContain('Delete');
    expect(texts.some((t) => t.includes('Source'))).toBe(false);
    // The default keeps all three.
    const full = buttonTexts(renderWith(mem({ source: 'AUTO', chatId: 'chat-1', sourceMessageId: 'msg-1' })));
    expect(full).toContain('Edit');
    expect(full).toContain('Delete');
    expect(full.some((t) => t.includes('Source'))).toBe(true);
  });

  it('renders a pre-round row (no tier keys) exactly as before', () => {
    const fixture = renderWith(mem({ source: 'AUTO' }));
    expect(chips(fixture).map((c) => c.textContent?.trim())).toEqual(['Auto']);
    expect(provenance(fixture)).toBeNull();
    expect(((fixture.nativeElement as HTMLElement).querySelector('.qt-card') as HTMLElement).classList.contains('opacity-75')).toBe(false);
  });
});
