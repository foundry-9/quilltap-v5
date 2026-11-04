import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { TemplateAddOnsEditor } from './template-add-ons-editor';
import { EMPTY_ADD_ONS, type FullAddOns } from './template-form';

/** v4 `AddOnsEditor` (`roleplay-templates/index.tsx:44-110`, 778b97b2b). */
@Component({
  imports: [TemplateAddOnsEditor],
  template: `<qt-template-add-ons-editor [(addOns)]="addOns" />
    <qt-template-add-ons-editor [(addOns)]="other" />`,
})
class Host {
  readonly addOns = signal<FullAddOns>({ ...EMPTY_ADD_ONS });
  readonly other = signal<FullAddOns>({ ...EMPTY_ADD_ONS });
}

function mount() {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  return fixture;
}

describe('TemplateAddOnsEditor', () => {
  it('round-trips the six controls', () => {
    const fixture = mount();
    const first = (fixture.nativeElement as HTMLElement).querySelector('qt-template-add-ons-editor')!;
    const boxes = first.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
    expect(Array.from(boxes).map((b) => b.closest('label')!.textContent!.trim())).toEqual([
      'Bold',
      'Italic',
      'Reverse',
    ]);
    expect(boxes[2].closest('label')!.getAttribute('title')).toBe('Swap foreground and background');
    boxes[0].click();
    boxes[1].click();
    boxes[2].click();
    const selects = first.querySelectorAll<HTMLSelectElement>('select');
    for (const [i, value] of [
      [0, 'double'],
      [1, 'dashed'],
      [2, 'serif'],
    ] as const) {
      selects[i].value = value;
      selects[i].dispatchEvent(new Event('change'));
    }
    fixture.detectChanges();
    expect(fixture.componentInstance.addOns()).toEqual({
      bold: true,
      italic: true,
      reverse: true,
      underline: 'double',
      border: 'dashed',
      font: 'serif',
    });
    // The second instance is untouched.
    expect(fixture.componentInstance.other()).toEqual(EMPTY_ADD_ONS);
  });

  it('pairs each select with its visible label by a per-instance id (R4)', () => {
    const fixture = mount();
    const root = fixture.nativeElement as HTMLElement;
    const selects = Array.from(root.querySelectorAll<HTMLSelectElement>('select'));
    expect(selects).toHaveLength(6);
    expect(new Set(selects.map((s) => s.id)).size).toBe(6);
    expect(
      selects.slice(0, 3).map((s) => root.querySelector(`label[for="${s.id}"]`)!.textContent!.trim()),
    ).toEqual(['Underline', 'Border', 'Font']);
  });
});
