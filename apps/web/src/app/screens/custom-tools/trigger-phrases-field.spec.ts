import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { DraftIssue } from '../../pascal/tool-draft';
import { TRIGGER_PHRASES_HINT, TriggerPhrasesField } from './trigger-phrases-field';

/**
 * TriggerPhrasesField — asserted against v4
 * `components/custom-tools/TriggerPhrasesField.tsx` (`362426c8f`, pinned at
 * `25b8fab6a`), every user-visible string byte-pinned: Enter adds the TRIMMED
 * candidate and clears the input; Add is dead under three characters and at
 * the cap; the placeholder flips and the input disables at eight; a per-index
 * error renders as the chip's title AND a line; a list error as a line only;
 * a chip's × removes THAT index; `disabled` kills every control.
 */

async function render(
  triggers: string[],
  issues: DraftIssue[] = [],
  disabled = false,
): Promise<{ fixture: ComponentFixture<TriggerPhrasesField>; emitted: string[][] }> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [TriggerPhrasesField] });
  const fixture = TestBed.createComponent(TriggerPhrasesField);
  fixture.componentRef.setInput('triggers', triggers);
  fixture.componentRef.setInput('issues', issues);
  fixture.componentRef.setInput('disabled', disabled);
  const emitted: string[][] = [];
  fixture.componentInstance.triggersChange.subscribe((next) => emitted.push(next));
  fixture.detectChanges();
  return { fixture, emitted };
}

const root = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const input = (f: ComponentFixture<unknown>) => root(f).querySelector<HTMLInputElement>('#wb-trigger-phrase')!;
const addButton = (f: ComponentFixture<unknown>) =>
  [...root(f).querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent?.trim() === 'Add')!;

function type(f: ComponentFixture<unknown>, value: string): void {
  input(f).value = value;
  input(f).dispatchEvent(new Event('input'));
  f.detectChanges();
}

const err = (message: string, index?: number): DraftIssue => ({
  severity: 'error',
  message,
  where: index === undefined ? { section: 'triggers' } : { section: 'triggers', index },
});

describe('TriggerPhrasesField (v4 TriggerPhrasesField.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('labels the input and pins v4’s hint bytes', async () => {
    const { fixture } = await render([]);
    const label = root(fixture).querySelector('label');
    expect(label?.getAttribute('for')).toBe('wb-trigger-phrase');
    expect(label?.textContent?.trim()).toBe('Trigger phrases');
    expect(TRIGGER_PHRASES_HINT).toBe(
      'Said by anyone in the chat, these words run the tool with its defaults — only when the chat setting is on. Exact words, any case. 3–80 characters each, up to 8.',
    );
    expect(root(fixture).querySelector('.qt-hint')?.textContent?.trim()).toBe(TRIGGER_PHRASES_HINT);
    expect(input(fixture).getAttribute('maxlength')).toBe('80');
    expect(input(fixture).placeholder).toBe('abracadabra');
    // No chip list while empty.
    expect(root(fixture).querySelector('ul')).toBeNull();
  });

  it('Enter adds the TRIMMED candidate, clears the input, and prevents the form’s Enter', async () => {
    const { fixture, emitted } = await render(['open sesame']);
    type(fixture, '  abracadabra  ');
    const enter = new KeyboardEvent('keydown', { key: 'Enter', cancelable: true });
    input(fixture).dispatchEvent(enter);
    expect(enter.defaultPrevented).toBe(true);
    expect(emitted).toEqual([['open sesame', 'abracadabra']]);
    expect(input(fixture).value).toBe('');
  });

  it('two Enters before the parent re-renders keep BOTH phrases (no stale-input overwrite)', async () => {
    // The parent echoes each emission back through `triggers` only on its next
    // render; a second Enter before that render must build on the FIRST add,
    // not on the stale input (the e2e cap beat lost chips at the `25b8fab6a`
    // unification). No detectChanges between the two adds.
    const { fixture, emitted } = await render(['open sesame']);
    const el = input(fixture);
    el.value = 'abracadabra';
    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', cancelable: true }));
    el.value = 'hocus pocus';
    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', cancelable: true }));
    expect(emitted).toEqual([
      ['open sesame', 'abracadabra'],
      ['open sesame', 'abracadabra', 'hocus pocus'],
    ]);
  });

  it('reads the input element at call time, not a signal that may lag', async () => {
    const { fixture, emitted } = await render([]);
    // Set the value with NO input event — the pending signal never sees it.
    input(fixture).value = 'hocus pocus';
    input(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(emitted).toEqual([['hocus pocus']]);
  });

  it('keeps Add dead under three characters and refuses a short Enter', async () => {
    const { fixture, emitted } = await render([]);
    type(fixture, 'ok');
    expect(addButton(fixture).disabled).toBe(true);
    input(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(emitted).toEqual([]);
    type(fixture, ' ok ');
    expect(addButton(fixture).disabled).toBe(true);
    type(fixture, 'abc');
    expect(addButton(fixture).disabled).toBe(false);
    addButton(fixture).click();
    expect(emitted).toEqual([['abc']]);
  });

  it('flips the placeholder and disables the input and Add at the cap of eight', async () => {
    const eight = Array.from({ length: 8 }, (_, i) => `phrase ${i}`);
    const { fixture } = await render(eight);
    expect(input(fixture).placeholder).toBe('At most 8 phrases');
    expect(input(fixture).disabled).toBe(true);
    expect(addButton(fixture).disabled).toBe(true);
    const seven = await render(eight.slice(1));
    expect(input(seven.fixture).placeholder).toBe('abracadabra');
    expect(input(seven.fixture).disabled).toBe(false);
  });

  it('renders each chip with its remove button, and × removes THAT index (not a same-text twin)', async () => {
    const { fixture, emitted } = await render(['hocus', 'pocus', 'hocus']);
    const list = root(fixture).querySelector('ul');
    expect(list?.getAttribute('aria-label')).toBe('Trigger phrases');
    const chips = [...root(fixture).querySelectorAll('li')];
    expect(chips.map((c) => c.querySelector('span')?.textContent)).toEqual(['hocus', 'pocus', 'hocus']);
    expect(chips[0].className).toContain('qt-badge-secondary');
    const removes = [...root(fixture).querySelectorAll<HTMLButtonElement>('li button')];
    expect(removes.map((b) => b.getAttribute('aria-label'))).toEqual([
      'Remove trigger phrase hocus',
      'Remove trigger phrase pocus',
      'Remove trigger phrase hocus',
    ]);
    expect(removes[0].textContent?.trim()).toBe('×');
    removes[2].click();
    expect(emitted).toEqual([['hocus', 'pocus']]);
  });

  it('shows a per-index error as the chip’s title AND a line, and colours only that chip', async () => {
    const message = 'a trigger phrase must not begin or end with a space';
    const { fixture } = await render(['abracadabra', ' padded'], [err(message, 1)]);
    const chips = [...root(fixture).querySelectorAll('li')];
    expect(chips[1].getAttribute('title')).toBe(message);
    expect(chips[1].classList.contains('qt-text-destructive')).toBe(true);
    expect(chips[0].hasAttribute('title')).toBe(false);
    expect(chips[0].classList.contains('qt-text-destructive')).toBe(false);
    const lines = [...root(fixture).querySelectorAll('p.qt-text-destructive')].map((p) => p.textContent?.trim());
    expect(lines).toEqual([message]);
  });

  it('shows a list error as a line only, before the per-index lines, and ignores warnings', async () => {
    const issues: DraftIssue[] = [
      err('a trigger phrase needs at least 3 characters', 0),
      err('at most 8 trigger phrases'),
      { severity: 'warning', message: 'not an error', where: { section: 'triggers', index: 0 } },
      { severity: 'error', message: 'elsewhere', where: { section: 'identity', field: 'name' } },
    ];
    const { fixture } = await render(['ok'], issues);
    const lines = [...root(fixture).querySelectorAll('p.qt-text-destructive')].map((p) => p.textContent?.trim());
    expect(lines).toEqual(['at most 8 trigger phrases', 'a trigger phrase needs at least 3 characters']);
    expect(root(fixture).querySelector('li')?.getAttribute('title')).toBe(
      'a trigger phrase needs at least 3 characters',
    );
    for (const p of root(fixture).querySelectorAll('p.qt-text-destructive')) {
      expect(p.className).toBe('text-xs qt-text-destructive mt-1');
    }
  });

  it('disables Add, the input and every × when disabled', async () => {
    const { fixture, emitted } = await render(['hocus'], [], true);
    type(fixture, 'abracadabra');
    expect(input(fixture).disabled).toBe(true);
    expect(addButton(fixture).disabled).toBe(true);
    expect(root(fixture).querySelector<HTMLButtonElement>('li button')!.disabled).toBe(true);
    input(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(emitted).toEqual([]);
  });
});
