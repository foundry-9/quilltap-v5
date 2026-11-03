/** v4 `wardrobe-item-editor/WardrobePronounGuidanceField.tsx` at `236c04e35`. */
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { PronounGuidance } from '../../core/core-contract';
import { WardrobePronounGuidanceField } from './wardrobe-pronoun-guidance-field';

function render(value: PronounGuidance[]) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [WardrobePronounGuidanceField] });
  const fixture = TestBed.createComponent(WardrobePronounGuidanceField);
  fixture.componentRef.setInput('value', value);
  const emitted: PronounGuidance[][] = [];
  fixture.componentInstance.valueChange.subscribe((v) => emitted.push(v));
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement, emitted };
}

describe('WardrobePronounGuidanceField', () => {
  it('renders the legend, four boxes in picker order and v4’s paragraph', () => {
    const { el } = render(['she']);
    expect(el.querySelector('legend')!.className).toBe('qt-label mb-1');
    expect(el.querySelector('legend')!.textContent!.trim()).toBe('Pronoun Guidance');
    const labels = [...el.querySelectorAll('label')];
    expect(labels.map((l) => l.textContent!.trim())).toEqual([
      'He/Him/His',
      'She/Her/Her',
      'They/Them/Their',
      'It/It/Its',
    ]);
    expect(labels[0].className).toBe('flex items-center gap-2 text-sm text-foreground');
    expect(labels.map((l) => l.querySelector('input')!.checked)).toEqual([
      false,
      true,
      false,
      false,
    ]);
    expect(labels[0].querySelector('input')!.className).toBe('qt-checkbox');
    const p = el.querySelector('p')!;
    expect(p.className).toBe('mt-1 text-xs qt-text-small');
    expect(p.textContent!.replace(/\s+/g, ' ').trim()).toBe(
      "For whom is this piece cut? Tick the pronouns of the characters it is meant for, and the wardrobe will tactfully keep it out of everyone else's offerings --- no gentleman need be handed a ball gown he never asked for. Leave every box empty and it is offered to all. Anyone may still wear it if they ask for it by name.",
    );
  });

  it('emits the NORMALISED list — ticking it with he already ticked emits [he, it]', () => {
    const { el, emitted } = render(['it']);
    const he = el.querySelectorAll('input')[0] as HTMLInputElement;
    he.checked = true;
    he.dispatchEvent(new Event('change'));
    expect(emitted).toEqual([['he', 'it']]);
  });

  it('unticking removes the subject', () => {
    const { el, emitted } = render(['he', 'she']);
    const she = el.querySelectorAll('input')[1] as HTMLInputElement;
    she.checked = false;
    she.dispatchEvent(new Event('change'));
    expect(emitted).toEqual([['he']]);
  });
});
