import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Subject } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { PROMPT_FIELD_HINTS } from '../../../../ui/prompt-field-hints';
import type { GeneratableField, GeneratedCharacterData } from '../edit-generators.api';
import { WizardGenerationStep } from './generation-step';
import { WizardState } from './wizard-state';
import GENERATED from './fixtures/wizard-review-generated-data.json';

/**
 * The wizard review pane's expanded row against v4 `components/characters/
 * ai-wizard/steps/GenerationStep.tsx` at `2f4254b42` — v4 has no jest suite for
 * this component (measured: no `steps/__tests__/`), so the spec drives the
 * rendered control: it clicks the row's toggle button and reads the DOM, rather
 * than calling the render helpers directly (the `2f4254b42` round's lesson —
 * handler-direct specs leave template bindings unpinned).
 *
 * Three renders are pinned, all recorded as MISSING by the `p4.9k` round:
 *  - `:379` the `Written as:` voice hint above every hinted field;
 *  - `:103-148` the physical-description tier panel (teaser, four labelled
 *    tiers with UTF-16 char counts, then `fullDescription` as Markdown);
 *  - `:151-163` scenarios as `<strong>title</strong>` + pre-wrapped content.
 */

class FakeCore {
  readonly events$ = new Subject<Record<string, unknown>>().asObservable();
  async dispatchData(): Promise<Record<string, unknown>> {
    return {};
  }
}

const DATA = GENERATED as unknown as GeneratedCharacterData;

async function render(
  fields: GeneratableField[],
): Promise<ComponentFixture<WizardGenerationStep>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WizardGenerationStep],
    providers: [WizardState, { provide: CoreClient, useValue: new FakeCore() }],
  });
  const wizard = TestBed.inject(WizardState);
  wizard.configure({
    characterId: signal<string | undefined>('c1'),
    characterName: signal('Perpetua'),
    currentData: signal({}),
    onApply: () => {},
    onClose: () => {},
  });
  wizard.selectedFields.set(new Set(fields));
  wizard.generatedData.set(DATA);
  wizard.generating.set(false);
  const fixture = TestBed.createComponent(WizardGenerationStep);
  fixture.detectChanges();
  return fixture;
}

/** Expand the (single-field) review row the way the operator does. */
function expand(fixture: ComponentFixture<WizardGenerationStep>): HTMLElement {
  const el = fixture.nativeElement as HTMLElement;
  const toggle = el.querySelector('button') as HTMLButtonElement;
  toggle.click();
  fixture.detectChanges();
  return el;
}

describe('WizardGenerationStep — the expanded review row (v4 GenerationStep.tsx)', () => {
  it('renders the voice hint above a hinted field (v4 :379)', async () => {
    const el = expand(await render(['identity']));
    const example = el.querySelector('qt-prompt-field-example p') as HTMLParagraphElement;
    expect(example).toBeTruthy();
    // v4 maps `identity` → the `identity` hint, and PromptFieldExample takes its
    // default classes at this call site (no `className` prop at :379).
    expect(example.getAttribute('class')).toBe('text-xs qt-text-secondary');
    expect(example.textContent).toBe(`Written as: ${PROMPT_FIELD_HINTS.identity.example}`);
  });

  it('renders NO voice hint for a field with no hint key (v4 `hintKey ? … : undefined`)', async () => {
    // `wardrobeItems` is absent from v4's FIELD_HINT_KEYS.
    const data = { ...DATA, wardrobeItems: [{ title: 'Coat', description: '', types: ['top'] }] };
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [WizardGenerationStep],
      providers: [WizardState, { provide: CoreClient, useValue: new FakeCore() }],
    });
    const wizard = TestBed.inject(WizardState);
    wizard.configure({
      characterId: signal<string | undefined>('c1'),
      characterName: signal('Perpetua'),
      currentData: signal({}),
      onApply: () => {},
      onClose: () => {},
    });
    wizard.selectedFields.set(new Set<GeneratableField>(['wardrobeItems']));
    wizard.generatedData.set(data as unknown as GeneratedCharacterData);
    const fixture = TestBed.createComponent(WizardGenerationStep);
    fixture.detectChanges();
    const el = expand(fixture);
    expect(el.querySelector('qt-prompt-field-example')).toBeNull();
    expect(el.textContent).toContain('Coat');
  });

  it('renders the physical-description tier panel (v4 :103-148)', async () => {
    const el = expand(await render(['physicalDescription']));
    const pd = DATA.physicalDescription!;

    // The collapsed-style teaser: the first 100 UTF-16 units plus a literal "...".
    const teaser = el.querySelector('.mt-2.space-y-3 > .text-xs') as HTMLElement;
    expect(teaser.querySelector('strong')?.textContent).toBe('Short prompt:');
    expect(teaser.textContent).toBe(`Short prompt: ${pd.shortPrompt.substring(0, 100)}...`);
    // The fixture's short prompt is longer than the cut, so the teaser truncates.
    expect(pd.shortPrompt.length).toBeGreaterThan(100);
    expect(teaser.textContent).not.toContain(pd.shortPrompt);

    // The four tiers, in v4's order, each labelled with its UTF-16 length.
    const tiers = Array.from(el.querySelectorAll('.space-y-2.text-sm > div')).slice(0, 4);
    expect(tiers.map((d) => d.querySelector('strong')!.textContent)).toEqual([
      `Short (${pd.shortPrompt.length} chars):`,
      `Medium (${pd.mediumPrompt.length} chars):`,
      `Long (${pd.longPrompt.length} chars):`,
      `Complete (${pd.completePrompt.length} chars):`,
    ]);
    expect(tiers.map((d) => d.querySelector('p')!.textContent)).toEqual([
      pd.shortPrompt,
      pd.mediumPrompt,
      pd.longPrompt,
      pd.completePrompt,
    ]);
    // Four DIFFERENT lengths, so a copy-paste of the wrong tier would redden.
    expect(new Set(tiers.map((d) => d.querySelector('strong')!.textContent)).size).toBe(4);
    for (const d of tiers) {
      expect(d.querySelector('strong')!.getAttribute('class')).toBe('text-foreground');
      expect(d.querySelector('p')!.getAttribute('class')).toBe('qt-text-secondary');
    }
  });

  it('renders `fullDescription` as CommonMark, not plain text (v4 :127-140)', async () => {
    const el = expand(await render(['physicalDescription']));
    const labels = Array.from(el.querySelectorAll('strong')).map((s) => s.textContent);
    expect(labels).toContain('Full Description:');
    const prose = el.querySelector('.prose.prose-sm') as HTMLElement;
    expect(prose).toBeTruthy();
    expect(prose.getAttribute('class')).toBe('prose prose-sm qt-prose-auto max-w-none mt-1');
    // Markdown, not the source text.
    expect(prose.querySelector('h2')?.textContent).toBe('Appearance');
    expect(prose.querySelector('strong')?.textContent).toBe('taller');
    expect(prose.innerHTML).not.toContain('## Appearance');
    // The qtap href survives Angular's URL sanitizer (bypassSecurityTrustHtml).
    expect(prose.querySelector('a')?.getAttribute('href')).toBe('qtap://project/ledger.md');
    // Raw HTML dropped; GFM off (the two safety/pipeline properties).
    expect(prose.querySelector('b')).toBeNull();
    expect(prose.querySelector('table')).toBeNull();
    expect(prose.querySelector('del')).toBeNull();
  });

  it('renders scenarios as title + pre-wrapped content (v4 :151-163)', async () => {
    const el = expand(await render(['scenarios']));
    const rows = Array.from(el.querySelectorAll('.mt-2.space-y-2 > .text-sm'));
    expect(rows).toHaveLength(2);
    expect(rows.map((r) => r.querySelector('strong')!.textContent)).toEqual([
      'The Ledger Room',
      'The Airship Deck',
    ]);
    const first = rows[0].querySelector('p') as HTMLParagraphElement;
    expect(first.getAttribute('class')).toBe('qt-text-secondary mt-0.5 whitespace-pre-wrap');
    expect(first.textContent).toBe('Line one.\nLine two, after a hard break.');
    // The pane must NOT show the `**title**\ncontent` join `getFieldContent`
    // builds for the collapsed teaser — that stays the teaser's shape on both
    // sides (v4 :54-58 / v5 `fieldContent`).
    expect(el.textContent).not.toContain('**The Ledger Room**');
  });

  it('falls back to plain pre-wrapped text for every other field (v4 :165-170)', async () => {
    const el = expand(await render(['identity']));
    const plain = el.querySelector('.text-sm.qt-text-secondary.whitespace-pre-wrap') as HTMLElement;
    expect(plain).toBeTruthy();
    expect(plain.textContent).toBe(DATA.identity);
    expect(el.querySelector('.prose')).toBeNull();
  });
});
