import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { SelectLlmProfileDialog } from './select-llm-profile-dialog';

/** Hand Off Character to AI (v4 `SelectLLMProfileDialog.tsx`). */

let profiles: { id: string; name: string; provider?: string; modelName?: string }[] = [];

function stubClient(): Partial<CoreClient> {
  return {
    dispatchData: (async () => ({ profiles })) as unknown as CoreClient['dispatchData'],
  };
}

@Component({
  imports: [SelectLlmProfileDialog],
  template: `
    <qt-select-llm-profile-dialog
      [characterName]="'Riya'"
      [defaultConnectionProfileId]="preferred()"
      (confirm)="chosen.push($event)"
      (cancel)="cancelled = cancelled + 1"
    />
  `,
})
class Host {
  readonly preferred = signal<string | null>(null);
  readonly chosen: string[] = [];
  cancelled = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: stubClient() },
      provideTanStackQuery(new QueryClient()),
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function radios(fixture: ComponentFixture<Host>): HTMLInputElement[] {
  return Array.from(
    fixture.nativeElement.querySelectorAll('input[type="radio"]'),
  ) as HTMLInputElement[];
}

function handOff(fixture: ComponentFixture<Host>): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.includes('Hand Off'),
  )!;
}

describe('SelectLlmProfileDialog', () => {
  beforeEach(() => {
    profiles = [
      { id: 'cp1', name: 'Cheap and Cheerful', provider: 'openai', modelName: 'gpt-mini' },
      { id: 'cp2', name: 'The Good Stuff', provider: 'anthropic', modelName: 'opus' },
    ];
  });

  it('preselects the FIRST profile when the character has no default', async () => {
    const fixture = await render();
    expect(radios(fixture).map((r) => r.checked)).toEqual([true, false]);
    expect(handOff(fixture).disabled).toBe(false);
  });

  it('prefers the character’s own default over the first', async () => {
    const fixture = await render();
    fixture.componentInstance.preferred.set('cp2');
    fixture.detectChanges();
    expect(radios(fixture).map((r) => r.checked)).toEqual([false, true]);
  });

  it('reports the chosen profile to the parent', async () => {
    const fixture = await render();
    radios(fixture)[1].dispatchEvent(new Event('change'));
    fixture.detectChanges();
    handOff(fixture).click();
    expect(fixture.componentInstance.chosen).toEqual(['cp2']);
  });

  it('names the fix rather than offering an inert confirm when there are no profiles', async () => {
    profiles = [];
    const fixture = await render();
    expect(fixture.nativeElement.textContent).toContain('No connection profiles available');
    expect(fixture.nativeElement.textContent).toContain('Settings → Connection Profiles');
    expect(handOff(fixture).disabled).toBe(true);
  });

  it('treats the close button as a cancel, not a dismissal', async () => {
    const fixture = await render();
    (fixture.nativeElement.querySelector('[aria-label="Close"]') as HTMLButtonElement).click();
    expect(fixture.componentInstance.cancelled).toBe(1);
    expect(fixture.componentInstance.chosen).toEqual([]);
  });
});
