import { Component, signal } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { SpeakerSelector, type ControlledCharacter } from './speaker-selector';

@Component({
  imports: [SpeakerSelector],
  template: `
    <qt-speaker-selector
      [characters]="characters()"
      [activeParticipantId]="active()"
      (select)="selected.set($event)"
    />
  `,
})
class Host {
  readonly characters = signal<ControlledCharacter[]>([]);
  readonly active = signal<string | null>(null);
  readonly selected = signal<string | null>(null);
}

function render(): ComponentFixture<Host> {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  return fixture;
}

describe('SpeakerSelector', () => {
  it('renders nothing with fewer than two characters', () => {
    const fixture = render();
    fixture.componentInstance.characters.set([{ participantId: 'p1', name: 'Alice' }]);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('button')).toBeFalsy();
  });

  it('shows "Speaking as {active}" and emits the picked participant', () => {
    const fixture = render();
    fixture.componentInstance.characters.set([
      { participantId: 'p1', name: 'Alice' },
      { participantId: 'p2', name: 'Bob' },
    ]);
    fixture.componentInstance.active.set('p1');
    fixture.detectChanges();

    const toggle = fixture.nativeElement.querySelector('button') as HTMLButtonElement;
    expect(toggle.textContent).toContain('Speaking as Alice');

    toggle.click();
    fixture.detectChanges();
    const bob = [...fixture.nativeElement.querySelectorAll('[role="option"]')].find((o) =>
      (o as HTMLElement).textContent?.includes('Bob'),
    ) as HTMLButtonElement;
    bob.click();
    fixture.detectChanges();

    expect(fixture.componentInstance.selected()).toBe('p2');
  });

  it('does not emit when re-selecting the already-active speaker', () => {
    const fixture = render();
    fixture.componentInstance.characters.set([
      { participantId: 'p1', name: 'Alice' },
      { participantId: 'p2', name: 'Bob' },
    ]);
    fixture.componentInstance.active.set('p1');
    fixture.detectChanges();

    (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
    fixture.detectChanges();
    const alice = [...fixture.nativeElement.querySelectorAll('[role="option"]')].find((o) =>
      (o as HTMLElement).textContent?.includes('Alice'),
    ) as HTMLButtonElement;
    alice.click();
    fixture.detectChanges();

    expect(fixture.componentInstance.selected()).toBeNull();
  });
});
