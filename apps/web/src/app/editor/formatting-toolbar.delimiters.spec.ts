import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import oracle from './__fixtures__/delimiter-oracle.json';
import { FormattingToolbar } from './formatting-toolbar';
import type { NarrationDelimiters, TemplateDelimiter } from '../core/core-contract';

/**
 * The toolbar's delimiter section, diffed against the buttons v4's REAL
 * `FormattingToolbar` rendered in jsdom (`apps/web/oracle/
 * delimiter-transforms.test.ts`, section D). The vectors carry each button's
 * label, `title` and `className` plus the toolbar's divider COUNT, which is
 * what pins the section's presence and its position in the rail: v4 renders
 * three dividers when a template resolves delimiters and two when it does not.
 *
 * `delimiter-transforms.spec.ts` proves the LIST is right; this proves the
 * markup around it is.
 *
 * @module editor/formatting-toolbar.delimiters.spec
 */

interface ButtonListVector {
  name: string;
  template: TemplateDelimiter[];
  narration: NarrationDelimiters | null;
  buttons: { label: string; title: string; className: string }[];
  dividers: number;
}

const vectors = (oracle as unknown as { buttonLists: ButtonListVector[] }).buttonLists;

describe('FormattingToolbar — the delimiter section, v4 rendered-DOM parity', () => {
  let fixture: ComponentFixture<FormattingToolbar>;

  beforeEach(() => {
    TestBed.configureTestingModule({ imports: [FormattingToolbar] });
    fixture = TestBed.createComponent(FormattingToolbar);
  });

  afterEach(() => TestBed.resetTestingModule());

  function render(v: ButtonListVector): HTMLElement {
    fixture.componentRef.setInput('delimiters', v.template);
    fixture.componentRef.setInput('narrationDelimiters', v.narration);
    // v4 renders its source toggle whenever `onToggleSource` is passed, and the
    // recorder's host passes one — so the divider counts only line up with the
    // toggle shown.
    fixture.componentRef.setInput('showSourceToggle', true);
    fixture.detectChanges();
    return fixture.nativeElement as HTMLElement;
  }

  for (const v of vectors) {
    it(`matches v4: ${v.name}`, () => {
      const root = render(v);
      const buttons = Array.from(
        root.querySelectorAll<HTMLButtonElement>('.qt-rp-annotation-button'),
      );

      expect(buttons.map((b) => b.textContent!.trim())).toEqual(v.buttons.map((b) => b.label));
      expect(buttons.map((b) => b.getAttribute('title'))).toEqual(v.buttons.map((b) => b.title));
      expect(buttons.map((b) => b.className)).toEqual(v.buttons.map((b) => b.className));
      expect(root.querySelectorAll('.qt-formatting-toolbar-divider').length).toBe(v.dividers);
    });
  }

  it('emits the pressed delimiter and never steals the selection', () => {
    const v = vectors.find((c) => c.buttons.length > 0)!;
    const root = render(v);
    const emitted: TemplateDelimiter[] = [];
    fixture.componentInstance.applyDelimiter.subscribe((d) => emitted.push(d));

    const first = root.querySelector<HTMLButtonElement>('.qt-rp-annotation-button')!;
    // v4's `preventFocusLoss`: mousedown is prevented so the editor keeps its
    // selection, which every one of these transforms operates on.
    const mousedown = new MouseEvent('mousedown', { cancelable: true, bubbles: true });
    first.dispatchEvent(mousedown);
    expect(mousedown.defaultPrevented).toBe(true);

    first.click();
    expect(emitted).toHaveLength(1);
    expect(emitted[0].buttonName).toBe(v.buttons[0].label);
  });

  it('disables the delimiter buttons with the rest of the toolbar', () => {
    const v = vectors.find((c) => c.buttons.length > 0)!;
    fixture.componentRef.setInput('disabled', true);
    const root = render(v);
    const buttons = Array.from(
      root.querySelectorAll<HTMLButtonElement>('.qt-rp-annotation-button'),
    );
    expect(buttons.length).toBeGreaterThan(0);
    expect(buttons.every((b) => b.disabled)).toBe(true);
  });

  it('renders no delimiter section for a host that passes neither input', () => {
    fixture.detectChanges();
    const root = fixture.nativeElement as HTMLElement;
    expect(root.querySelectorAll('.qt-rp-annotation-button')).toHaveLength(0);
  });
});
