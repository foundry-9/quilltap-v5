import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { newDraft, type ToolDraft } from '../../pascal/tool-draft';
import {
  InterventionToggle,
  ToolFieldInterventionToggle,
  isToolFieldShown,
  withToolFieldShown,
} from './intervention-toggle';

/**
 * The Workbench's raised hand (v4 `components/custom-tools/InterventionToggle.tsx`,
 * `839575d89`): a named button whose `aria-pressed` says whether the field is
 * shown in the hold dialog, its title saying which, and the tool-level wrapper
 * that writes only `false` and renders nothing while the master is off. v4
 * ships no test of this file alone; these are v5-authored from the hunk.
 */

function renderToggle(shown: boolean, disabled = false): ComponentFixture<InterventionToggle> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [InterventionToggle] });
  const fixture = TestBed.createComponent(InterventionToggle);
  fixture.componentRef.setInput('shown', shown);
  fixture.componentRef.setInput('label', 'Show the roll in the hold dialog');
  fixture.componentRef.setInput('disabled', disabled);
  fixture.detectChanges();
  return fixture;
}

const button = (fixture: ComponentFixture<unknown>) =>
  (fixture.nativeElement as HTMLElement).querySelector('button');

function held(enabled: boolean, fields: ToolDraft['intervention']['fields'] = {}): ToolDraft {
  return { ...newDraft(), intervention: { enabled, authored: false, fields } };
}

describe('InterventionToggle', () => {
  it('is a named button pressed while the field is shown, titled "(shown)"', () => {
    const b = button(renderToggle(true))!;
    expect(b.getAttribute('type')).toBe('button');
    expect(b.getAttribute('aria-label')).toBe('Show the roll in the hold dialog');
    expect(b.getAttribute('aria-pressed')).toBe('true');
    expect(b.getAttribute('title')).toBe('Show the roll in the hold dialog (shown)');
    expect(b.className).toBe('qt-intervene-toggle');
    expect(b.querySelector('[data-icon="hand-raised"]')?.getAttribute('aria-hidden')).toBe('true');
  });

  it('is struck through (--off) and titled "(hidden)" while the field is hidden', () => {
    const b = button(renderToggle(false))!;
    expect(b.getAttribute('aria-pressed')).toBe('false');
    expect(b.getAttribute('title')).toBe('Show the roll in the hold dialog (hidden)');
    expect(b.classList.contains('qt-intervene-toggle--off')).toBe(true);
  });

  it('emits the opposite of shown on a click, and is inert while disabled', () => {
    const fixture = renderToggle(true);
    const seen: boolean[] = [];
    fixture.componentInstance.shownChange.subscribe((v) => seen.push(v));
    button(fixture)!.click();
    expect(seen).toEqual([false]);

    const off = renderToggle(false, true);
    expect(button(off)!.disabled).toBe(true);
  });
});

describe('isToolFieldShown / withToolFieldShown', () => {
  it('reads an absent key as shown and only false as hidden', () => {
    expect(isToolFieldShown(held(true), 'roll')).toBe(true);
    expect(isToolFieldShown(held(true, { roll: true }), 'roll')).toBe(true);
    expect(isToolFieldShown(held(true, { roll: false }), 'roll')).toBe(false);
  });

  it('writes false on hide and DELETES the key on show, leaving the rest of the draft alone', () => {
    const draft = held(false, { title: false });
    const hidden = withToolFieldShown(draft, 'roll', false);
    expect(hidden.intervention).toEqual({ enabled: false, authored: false, fields: { title: false, roll: false } });
    const shown = withToolFieldShown(hidden, 'roll', true);
    expect('roll' in shown.intervention.fields).toBe(false);
    expect(shown.intervention.fields.title).toBe(false);
    expect(draft.intervention.fields).toEqual({ title: false });
  });
});

describe('ToolFieldInterventionToggle', () => {
  function renderField(draft: ToolDraft): ComponentFixture<ToolFieldInterventionToggle> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [ToolFieldInterventionToggle] });
    const fixture = TestBed.createComponent(ToolFieldInterventionToggle);
    fixture.componentRef.setInput('draft', draft);
    fixture.componentRef.setInput('fieldKey', 'roll');
    fixture.componentRef.setInput('label', 'Show the roll in the hold dialog');
    fixture.detectChanges();
    return fixture;
  }

  it('renders nothing while the master is off', () => {
    expect(button(renderField(held(false, { roll: false })))).toBeNull();
  });

  it('binds the hand to the draft and emits the next draft', () => {
    const fixture = renderField(held(true, { roll: false }));
    expect(button(fixture)!.getAttribute('aria-pressed')).toBe('false');
    const seen: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => seen.push(d));
    button(fixture)!.click();
    expect(seen[0]!.intervention.fields).toEqual({});
  });
});
