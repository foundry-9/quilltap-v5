import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { OrganizeSection } from './organize-section';

/** The Organize drawer's entries and their gate (v4 `OrganizeSection`). */

@Component({
  imports: [OrganizeSection],
  template: `
    <qt-organize-section
      [chatId]="'chat-1'"
      [isAutonomousRoom]="isAutonomousRoom()"
      (editEnclave)="fired.push('enclave')"
      (rename)="fired.push('rename')"
      (mergeIn)="fired.push('merge')"
      (openState)="fired.push('state')"
      (openGallery)="fired.push('gallery')"
    />
  `,
})
class Host {
  readonly isAutonomousRoom = signal(false);
  readonly fired: string[] = [];
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function labels(fixture: ComponentFixture<Host>): string[] {
  return Array.from(fixture.nativeElement.querySelectorAll('button')).map((b) =>
    (b as HTMLButtonElement).textContent!.trim(),
  );
}

describe('OrganizeSection', () => {
  it('shows Copy ID, State and Gallery — and Edit Enclave only for an autonomous room', async () => {
    const fixture = await render();
    expect(labels(fixture)).toEqual([
      'Copy ID',
      'Rename',
      'State…',
      'Merge In…',
      'Export',
      'Gallery',
    ]);

    fixture.componentInstance.isAutonomousRoom.set(true);
    fixture.detectChanges();
    // Merge In… is HIDDEN in an autonomous room (v4 :1566 `&& !isAutonomousRoom`).
    expect(labels(fixture)).toEqual([
      'Edit Enclave',
      'Copy ID',
      'Rename',
      'State…',
      'Export',
      'Gallery',
    ]);
  });

  it('reports each entry to the Salon', async () => {
    const fixture = await render();
    fixture.componentInstance.isAutonomousRoom.set(true);
    fixture.detectChanges();

    for (const label of ['Edit Enclave', 'Rename', 'State…', 'Gallery']) {
      const button = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
        (b) => (b as HTMLButtonElement).textContent!.trim() === label,
      ) as HTMLButtonElement;
      button.click();
    }
    expect(fixture.componentInstance.fired).toEqual(['enclave', 'rename', 'state', 'gallery']);
  });
});
