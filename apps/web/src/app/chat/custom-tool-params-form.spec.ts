import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { CustomToolParameterSpec } from '../core/core-contract';
import {
  CustomToolParamsForm,
  boundsHint,
  coerceParamValues,
  initialParamValues,
  type ParameterFormValues,
} from './custom-tool-params-form';

const PARAMS: Record<string, CustomToolParameterSpec> = {
  scale: { type: 'integer', default: 3, min: 1, max: 6, description: 'how hard' },
  weight: { type: 'number', default: 1.5 },
  label: { type: 'string', default: 'lock' },
  loud: { type: 'boolean', default: true },
};

describe('custom-tool-params-form — coercion (v4 CustomToolParamsForm)', () => {
  it('seeds the form from the declared defaults, booleans as booleans', () => {
    expect(initialParamValues(PARAMS)).toEqual({
      scale: '3',
      weight: '1.5',
      label: 'lock',
      loud: true,
    });
  });

  it('coerces text values back to the declared types', () => {
    const values: ParameterFormValues = { scale: '5', weight: '2.25', label: 'vault', loud: false };
    expect(coerceParamValues(PARAMS, values)).toEqual({
      scale: 5,
      weight: 2.25,
      label: 'vault',
      loud: false,
    });
  });

  it('falls back to the declared default for a blank or unparseable number (never NaN)', () => {
    const values: ParameterFormValues = { scale: '', weight: 'abc', label: '', loud: true };
    const out = coerceParamValues(PARAMS, values);
    expect(out['scale']).toBe(3); // blank integer → default
    expect(out['weight']).toBe(1.5); // unparseable float → default
    expect(out['label']).toBe(''); // an empty string stays an empty string
    expect(out['loud']).toBe(true);
  });

  it('parseInt truncates a float typed into an integer field', () => {
    expect(coerceParamValues(PARAMS, { ...initialParamValues(PARAMS), scale: '4.9' })['scale']).toBe(4);
  });
});

describe('CustomToolParamsForm — rendering', () => {
  afterEach(() => TestBed.resetTestingModule());

  function render(layout?: 'inline' | 'stacked'): ComponentFixture<CustomToolParamsForm> {
    TestBed.configureTestingModule({ imports: [CustomToolParamsForm] });
    const fixture = TestBed.createComponent(CustomToolParamsForm);
    fixture.componentRef.setInput('parameters', PARAMS);
    fixture.componentRef.setInput('values', initialParamValues(PARAMS));
    fixture.componentRef.setInput('idPrefix', 'pfx');
    if (layout) fixture.componentRef.setInput('layout', layout);
    fixture.detectChanges();
    return fixture;
  }

  it('renders a checkbox for a boolean and number/text inputs otherwise, with unique ids', () => {
    const el = render().nativeElement as HTMLElement;
    const scale = el.querySelector('#pfx-scale') as HTMLInputElement;
    const label = el.querySelector('#pfx-label') as HTMLInputElement;
    const loud = el.querySelector('#pfx-loud') as HTMLInputElement;
    expect(scale.type).toBe('number');
    expect(scale.getAttribute('min')).toBe('1');
    expect(scale.getAttribute('max')).toBe('6');
    expect(label.type).toBe('text');
    expect(label.hasAttribute('min')).toBe(false);
    expect(loud.type).toBe('checkbox');
    expect(loud.checked).toBe(true);
  });

  it('emits a ParamChange when a field changes', () => {
    const fixture = render();
    const seen: { param: string; value: string | boolean }[] = [];
    fixture.componentInstance.paramChange.subscribe((c) => seen.push(c));
    const label = fixture.nativeElement.querySelector('#pfx-label') as HTMLInputElement;
    label.value = 'vault';
    label.dispatchEvent(new Event('input'));
    expect(seen).toEqual([{ param: 'label', value: 'vault' }]);
  });

  // --- P4.d21 (v4 faab6881): the stacked dialog layout ----------------------

  it('defaults to inline, so the Workbench proving bench is untouched', () => {
    const implicit = (render().nativeElement as HTMLElement).innerHTML;
    TestBed.resetTestingModule();
    const el = render('inline').nativeElement as HTMLElement;
    // Explicit default and no-arg render must agree, element for element.
    expect(implicit).toBe(el.innerHTML);
    // A string parameter is a one-line text input in this layout, never a textarea.
    expect((el.querySelector('#pfx-label') as HTMLInputElement).tagName).toBe('INPUT');
    expect(el.querySelector('qt-auto-grow-textarea')).toBeNull();
    // The description stays hidden in a tooltip.
    expect(el.querySelector('label[for="pfx-scale"]')?.getAttribute('title')).toBe('how hard');
  });

  it('gives a stacked string parameter an auto-growing textarea, numbers a number input', () => {
    const el = render('stacked').nativeElement as HTMLElement;
    const label = el.querySelector('#pfx-label') as HTMLTextAreaElement;
    expect(label.tagName).toBe('TEXTAREA');
    expect(label.style.minHeight).toBe('40px');
    expect(label.style.maxHeight).toBe('224px');
    const scale = el.querySelector('#pfx-scale') as HTMLInputElement;
    expect(scale.tagName).toBe('INPUT');
    expect(scale.type).toBe('number');
    expect(scale.getAttribute('min')).toBe('1');
  });

  it('puts the label, the declared bounds, and the description on their own lines', () => {
    const el = render('stacked').nativeElement as HTMLElement;
    const label = el.querySelector('label[for="pfx-scale"]') as HTMLLabelElement;
    // The type and the bounds ride the label; the description gets its own line.
    expect(label.textContent?.replace(/\s+/g, ' ').trim()).toBe('scale integer · 1–6');
    expect(label.getAttribute('title')).toBeNull();
    expect(el.textContent).toContain('how hard');
  });

  it('emits from a stacked textarea the same way the inline input does', () => {
    const fixture = render('stacked');
    const seen: { param: string; value: string | boolean }[] = [];
    fixture.componentInstance.paramChange.subscribe((c) => seen.push(c));
    const label = fixture.nativeElement.querySelector('#pfx-label') as HTMLTextAreaElement;
    label.value = 'vault';
    label.dispatchEvent(new Event('input'));
    expect(seen).toEqual([{ param: 'label', value: 'vault' }]);
  });
});

describe('boundsHint (v4 faab6881)', () => {
  it('describes each declared shape, and nothing for a string or a boolean', () => {
    expect(boundsHint({ type: 'integer', default: 1, min: 1, max: 6 })).toBe('1–6');
    expect(boundsHint({ type: 'number', default: 1, min: 1 })).toBe('1 or more');
    expect(boundsHint({ type: 'number', default: 1, max: 6 })).toBe('6 or less');
    expect(boundsHint({ type: 'number', default: 1 })).toBeNull();
    expect(boundsHint({ type: 'string', default: 'x', min: 1, max: 6 })).toBeNull();
    expect(boundsHint({ type: 'boolean', default: true })).toBeNull();
  });
});
/**
 * Dogfood #51. `AutoGrowTextarea` and this form both once named their output
 * `change`, which is also a native DOM event that BUBBLES: the inner
 * `<textarea>`'s change (fired on blur — i.e. the moment you click Run) reached
 * the same binding and delivered an `Event` object on top of the good string.
 * The form then persisted it, so the next open showed `[object Event]` in every
 * field you had touched. The outputs are `valueChange` / `paramChange` now, and
 * these tests hold that line: a native bubbling change must emit NOTHING.
 */
describe('CustomToolParamsForm — native DOM events must not reach the outputs', () => {
  afterEach(() => TestBed.resetTestingModule());

  function renderStacked(): ComponentFixture<CustomToolParamsForm> {
    TestBed.configureTestingModule({ imports: [CustomToolParamsForm] });
    const fixture = TestBed.createComponent(CustomToolParamsForm);
    fixture.componentRef.setInput('parameters', PARAMS);
    fixture.componentRef.setInput('values', initialParamValues(PARAMS));
    fixture.componentRef.setInput('idPrefix', 'pfx');
    fixture.componentRef.setInput('layout', 'stacked');
    fixture.detectChanges();
    return fixture;
  }

  it('a blurred textarea emits nothing — no [object Event] reaches the values', () => {
    const fixture = renderStacked();
    const seen: { param: string; value: string | boolean }[] = [];
    fixture.componentInstance.paramChange.subscribe((c) => seen.push(c));

    const label = fixture.nativeElement.querySelector('#pfx-label') as HTMLTextAreaElement;
    label.value = 'vault';
    label.dispatchEvent(new Event('change', { bubbles: true }));

    expect(seen).toEqual([]);
  });

  it('typing still emits the string, so the guard did not silence the field', () => {
    const fixture = renderStacked();
    const seen: { param: string; value: string | boolean }[] = [];
    fixture.componentInstance.paramChange.subscribe((c) => seen.push(c));

    const label = fixture.nativeElement.querySelector('#pfx-label') as HTMLTextAreaElement;
    label.value = 'vault';
    label.dispatchEvent(new Event('input'));
    // Then blur, as a real run does: the value must survive untouched.
    label.dispatchEvent(new Event('change', { bubbles: true }));

    expect(seen).toEqual([{ param: 'label', value: 'vault' }]);
  });
});
