/**
 * ModelPicker — asserted against v4 `ModelPicker.tsx`: closed by default, the
 * button label reflects the active profile (else "Choose a model"), opening
 * lists every profile with provider·model and a check on the active one,
 * selecting emits the id and closes, and an empty/disabled picker disables the
 * button.
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { BrahmaConnectionProfile } from './brahma-console.service';
import { ModelPicker } from './model-picker';

const PROFILES: BrahmaConnectionProfile[] = [
  { id: 'p1', name: 'Everyday', provider: 'OPENAI', modelName: 'gpt-4o' },
  { id: 'p2', name: 'Deep', provider: 'ANTHROPIC', modelName: 'claude-opus-4-8' },
];

async function render(
  profiles: BrahmaConnectionProfile[],
  activeId: string | null = null,
): Promise<ComponentFixture<ModelPicker>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ModelPicker] });
  const fixture = TestBed.createComponent(ModelPicker);
  fixture.componentRef.setInput('profiles', profiles);
  fixture.componentRef.setInput('activeId', activeId);
  fixture.detectChanges();
  return fixture;
}

function button(fixture: ComponentFixture<ModelPicker>): HTMLButtonElement {
  return fixture.nativeElement.querySelector('button') as HTMLButtonElement;
}

function text(fixture: ComponentFixture<ModelPicker>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

describe('ModelPicker (v4 ModelPicker.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('is closed by default and shows "Choose a model" with no active profile', async () => {
    const fixture = await render(PROFILES);
    expect(text(fixture)).toContain('Choose a model');
    expect(fixture.nativeElement.querySelector('[role="listbox"]')).toBeNull();
  });

  it('labels the button with the active profile name', async () => {
    const fixture = await render(PROFILES, 'p2');
    expect(button(fixture).textContent).toContain('Deep');
  });

  it('opening lists every profile with provider·model', async () => {
    const fixture = await render(PROFILES);
    button(fixture).click();
    fixture.detectChanges();
    const listbox = fixture.nativeElement.querySelector('[role="listbox"]') as HTMLElement;
    expect(listbox).not.toBeNull();
    expect(listbox.textContent).toContain('Everyday');
    expect(listbox.textContent).toContain('OPENAI · gpt-4o');
    expect(listbox.textContent).toContain('claude-opus-4-8');
  });

  it('marks the active option aria-selected', async () => {
    const fixture = await render(PROFILES, 'p1');
    button(fixture).click();
    fixture.detectChanges();
    const options = fixture.nativeElement.querySelectorAll('[role="option"]');
    expect(options[0].getAttribute('aria-selected')).toBe('true');
    expect(options[1].getAttribute('aria-selected')).toBe('false');
  });

  it('selecting a profile emits its id and closes', async () => {
    const fixture = await render(PROFILES, 'p1');
    const picked: string[] = [];
    fixture.componentInstance.select.subscribe((id) => picked.push(id));

    button(fixture).click();
    fixture.detectChanges();
    const options = fixture.nativeElement.querySelectorAll('[role="option"]');
    (options[1] as HTMLButtonElement).click();
    fixture.detectChanges();

    expect(picked).toEqual(['p2']);
    expect(fixture.nativeElement.querySelector('[role="listbox"]')).toBeNull();
  });

  it('disables the trigger with no profiles', async () => {
    const fixture = await render([]);
    expect(button(fixture).disabled).toBe(true);
  });
});
