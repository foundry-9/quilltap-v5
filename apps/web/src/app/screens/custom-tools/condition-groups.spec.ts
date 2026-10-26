import { Component, TemplateRef, viewChild } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { ConditionJoiner } from '../../pascal/tool-draft';
import { ConditionGroups, JoinerToggle } from './condition-groups';

/**
 * ConditionGroups and JoinerToggle — v4 `ConditionGroups.tsx` (`a53024d3c`)
 * has no jsdom test of its own; these rows pin its markup byte for byte.
 */

interface Chip {
  id: string;
  joiner?: ConditionJoiner;
}

@Component({
  imports: [ConditionGroups],
  template: `
    <qt-condition-groups
      [conditions]="conditions"
      [disabled]="disabled"
      [chip]="chipTpl"
      (joinerChange)="changes.push($event)"
    />
    <ng-template #chipTpl let-condition><span class="chip">{{ condition.id }}</span></ng-template>
  `,
})
class Host {
  conditions: Chip[] = [];
  disabled = false;
  changes: Array<{ id: string; joiner: ConditionJoiner }> = [];
  readonly tpl = viewChild<TemplateRef<unknown>>('chipTpl');
}

function render(conditions: Chip[], disabled = false): ComponentFixture<Host> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.conditions = conditions;
  fixture.componentInstance.disabled = disabled;
  fixture.detectChanges();
  return fixture;
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const toggles = (f: ComponentFixture<unknown>) =>
  [...el(f).querySelectorAll<HTMLButtonElement>('qt-joiner-toggle button')];

describe('ConditionGroups (v4 ConditionGroups.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('draws one line with no toggle before the first chip and AND before the rest', () => {
    const f = render([{ id: 'a' }, { id: 'b' }, { id: 'c' }]);
    expect(toggles(f).map((b) => b.textContent?.trim())).toEqual(['AND', 'AND']);
    expect([...el(f).querySelectorAll('.chip')].map((c) => c.textContent)).toEqual(['a', 'b', 'c']);
    const lines = el(f).querySelectorAll('qt-condition-groups > div');
    expect(lines).toHaveLength(1);
    expect(lines[0].hasAttribute('class')).toBe(false);
    expect(lines[0].firstElementChild!.getAttribute('class')).toBe('flex items-center gap-2 flex-wrap');
  });

  it('draws two lines: the second carries the hairline and its first toggle reads OR', () => {
    const f = render([{ id: 'a' }, { id: 'b' }, { id: 'c', joiner: 'or' }, { id: 'd' }]);
    const lines = [...el(f).querySelectorAll('qt-condition-groups > div')];
    expect(lines.map((l) => l.getAttribute('class'))).toEqual([null, 'border-t qt-border-default pt-1 mt-1']);
    expect(toggles(f).map((b) => b.textContent?.trim())).toEqual(['AND', 'OR', 'AND']);
  });

  it('emits { id, joiner } with the OTHER joiner on a click', () => {
    const f = render([{ id: 'a' }, { id: 'b' }, { id: 'c', joiner: 'or' }]);
    const [first, second] = toggles(f);
    first.click();
    second.click();
    expect(f.componentInstance.changes).toEqual([
      { id: 'b', joiner: 'or' },
      { id: 'c', joiner: 'and' },
    ]);
  });

  it('passes disabled to every toggle', () => {
    const f = render([{ id: 'a' }, { id: 'b' }, { id: 'c', joiner: 'or' }], true);
    expect(toggles(f).every((b) => b.disabled)).toBe(true);
  });

  it('re-groups the lines when the input changes (a plain computed — no linkedSignal is needed)', () => {
    const f = render([{ id: 'a' }, { id: 'b' }]);
    expect(el(f).querySelectorAll('qt-condition-groups > div')).toHaveLength(1);
    f.componentInstance.conditions = [{ id: 'a' }, { id: 'b', joiner: 'or' }];
    f.changeDetectorRef.markForCheck();
    f.detectChanges();
    expect(el(f).querySelectorAll('qt-condition-groups > div')).toHaveLength(2);
  });

  it('renders nothing for an empty list', () => {
    expect(el(render([])).querySelectorAll('qt-condition-groups > div')).toHaveLength(0);
  });
});

describe('JoinerToggle (v4 JoinerToggle)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function toggle(joiner: ConditionJoiner): HTMLButtonElement {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [JoinerToggle] });
    const fixture = TestBed.createComponent(JoinerToggle);
    fixture.componentRef.setInput('joiner', joiner);
    fixture.detectChanges();
    return (fixture.nativeElement as HTMLElement).querySelector('button')!;
  }

  it('is a ghost button with the monospace classes', () => {
    expect(toggle('and').getAttribute('class')).toBe('qt-button qt-button-ghost qt-button-sm font-mono text-xs');
    expect(toggle('and').getAttribute('type')).toBe('button');
  });

  it('reads AND with its aria-label and title', () => {
    const b = toggle('and');
    expect(b.textContent?.trim()).toBe('AND');
    expect(b.getAttribute('aria-label')).toBe('Joined by AND — switch to OR');
    expect(b.getAttribute('title')).toBe(
      'AND keeps this test on the same line: every test on a line must hold. Click to start a new line.',
    );
  });

  it('reads OR with its aria-label and title', () => {
    const b = toggle('or');
    expect(b.textContent?.trim()).toBe('OR');
    expect(b.getAttribute('aria-label')).toBe('Joined by OR — switch to AND');
    expect(b.getAttribute('title')).toBe(
      'OR starts a new line: any line that holds will do. Click to fold it into the line above.',
    );
  });
});
